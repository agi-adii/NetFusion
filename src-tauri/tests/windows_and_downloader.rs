use netfusion_lib::{
    bonding::client::BondingTunnelClient,
    db::Database,
    downloader::engine::DownloadEngine,
    logger::AppLogger,
    models::{AddDownloadRequest, DownloadPriority, DownloadStatus},
    network::{monitor::NetworkMonitor, win_api::enumerate_windows_adapters},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn test_real_windows_adapter_enumeration_and_encrypted_vault() {
    let adapters = enumerate_windows_adapters();
    println!("Detected {} Windows network adapter(s):", adapters.len());
    for a in &adapters {
        println!(
            "  - [if_index={}] {} ({:?}) up={} ipv4={:?} link_bps={} rx={} tx={}",
            a.if_index, a.name, a.kind, a.is_up, a.ipv4, a.link_speed_bps, a.rx_octets, a.tx_octets
        );
    }
    assert!(
        !adapters.is_empty(),
        "Expected at least one Windows network adapter from GetAdaptersAddresses"
    );

    // Verify SQLite vault never stores PSK in plaintext
    let temp_dir = std::env::temp_dir().join(format!("netfusion_test_{}", uuid::Uuid::new_v4()));
    let db_file = temp_dir.join("vault_test.db");
    let db = Database::open(db_file.clone()).unwrap();
    let secret = "my-ultra-sensitive-bonding-secret-998877";
    db.store_bonding_psk(secret).unwrap();
    assert_eq!(db.load_bonding_psk(), secret);

    let raw_sqlite_bytes = std::fs::read(&db_file).unwrap();
    let raw_lossy = String::from_utf8_lossy(&raw_sqlite_bytes);
    assert!(
        !raw_lossy.contains(secret),
        "Security violation: plaintext secret must never appear in SQLite file"
    );
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_segmented_download_pause_resume_and_sha256_verification() {
    // 1. Spin up a real local HTTP/1.1 Range-capable file server serving a 3 MiB file
    let payload: Vec<u8> = (0..(3 * 1024 * 1024))
        .map(|i| ((i * 73 + 19) % 256) as u8)
        .collect();
    let mut sha = Sha256::new();
    sha.update(&payload);
    let expected_sha256 = hex::encode(sha.finalize());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let payload_srv = Arc::new(payload.clone());

    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let data = payload_srv.clone();
            tokio::spawn(async move {
                let mut req_buf = vec![0u8; 4096];
                let n = stream.read(&mut req_buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                let req_str = String::from_utf8_lossy(&req_buf[..n]);
                let first_line = req_str.lines().next().unwrap_or("");
                let is_head = first_line.starts_with("HEAD ");
                let total = data.len();

                let mut range_opt: Option<(usize, usize)> = None;
                for line in req_str.lines() {
                    if line.to_ascii_lowercase().starts_with("range: bytes=") {
                        let spec = line[13..].trim();
                        if let Some((s, e)) = spec.split_once('-') {
                            let start = s.parse::<usize>().unwrap_or(0);
                            let end = if e.is_empty() {
                                total - 1
                            } else {
                                e.parse::<usize>().unwrap_or(total - 1).min(total - 1)
                            };
                            if start <= end && start < total {
                                range_opt = Some((start, end));
                            }
                        }
                    }
                }

                if is_head {
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nAccept-Ranges: bytes\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n"
                    );
                    let _ = stream.write_all(hdr.as_bytes()).await;
                } else if let Some((start, end)) = range_opt {
                    let len = end - start + 1;
                    let hdr = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {len}\r\nContent-Range: bytes {start}-{end}/{total}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                    );
                    let _ = stream.write_all(hdr.as_bytes()).await;
                    for chunk in data[start..=end].chunks(64 * 1024) {
                        if stream.write_all(chunk).await.is_err() {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                } else {
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                    );
                    let _ = stream.write_all(hdr.as_bytes()).await;
                    let _ = stream.write_all(&data).await;
                }
                let _ = stream.shutdown().await;
            });
        }
    });

    // 2. Initialize Engine & start 6-segment parallel download
    let temp_dir = std::env::temp_dir().join(format!("netfusion_dl_{}", uuid::Uuid::new_v4()));
    let db = Arc::new(Database::open(temp_dir.join("dl.db")).unwrap());
    let logger = AppLogger::new(db.clone());
    let bonding = BondingTunnelClient::new(db.clone(), logger.clone());
    let monitor = NetworkMonitor::new(db.clone(), logger.clone(), bonding.clone());
    let engine = DownloadEngine::new(db.clone(), logger.clone(), monitor.clone(), bonding.clone());

    let url = format!("http://127.0.0.1:{port}/test_release.bin");
    let added = engine
        .add_download(AddDownloadRequest {
            url,
            file_name: Some("test_release.bin".into()),
            save_dir: Some(temp_dir.to_string_lossy().to_string()),
            priority: DownloadPriority::Critical,
            max_segments: Some(6),
            speed_limit_bps: None,
            scheduled_at_ms: None,
            expected_checksum: Some(expected_sha256.clone()),
            checksum_algorithm: Some("SHA256".into()),
            start_immediately: true,
        })
        .await
        .unwrap();

    // Pause briefly mid-flight once downloading starts, then resume!
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    engine.pause_download(&added.id).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    engine.resume_download(&added.id).await.unwrap();

    // Wait up to 12 seconds for completion
    let mut final_item = None;
    for _ in 0..120 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let list = engine.list_downloads().await;
        if let Some(it) = list.into_iter().find(|d| d.id == added.id) {
            if it.status == DownloadStatus::Completed || it.status == DownloadStatus::Failed {
                final_item = Some(it);
                break;
            }
        }
    }

    let completed = final_item.expect("Download should finish within timeout");
    assert_eq!(
        completed.status,
        DownloadStatus::Completed,
        "Download failed with: {:?}",
        completed.error_message
    );
    assert_eq!(completed.checksum_verified, Some(true));
    assert_eq!(
        completed.computed_checksum.as_deref(),
        Some(expected_sha256.as_str())
    );

    let disk_bytes = std::fs::read(&completed.target_path).unwrap();
    assert_eq!(disk_bytes.len(), payload.len());
    assert_eq!(disk_bytes, payload);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

