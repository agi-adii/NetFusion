use bonding_server::session::BondingSession;
use netfusion_protocol::{
    client_handshake, read_encrypted_frame, scheduler::StreamReassembler, server_handshake,
    write_encrypted_frame, BondingFrame,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

#[tokio::test]
async fn test_three_subflow_striping_and_midstream_failover() {
    // 1. Start a real origin payload server delivering 1.5 MiB of deterministic data
    let origin_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_addr = origin_listener.local_addr().unwrap();
    let expected_payload: Vec<u8> = (0..(1536 * 1024)).map(|i| ((i * 31 + 7) % 251) as u8).collect();
    let payload_for_origin = expected_payload.clone();

    tokio::spawn(async move {
        if let Ok((mut sock, _)) = origin_listener.accept().await {
            let mut req_buf = [0u8; 64];
            let _ = sock.read(&mut req_buf).await;
            // Stream in chunks with slight pacing so subflow drop happens mid-transfer
            for chunk in payload_for_origin.chunks(16 * 1024) {
                if sock.write_all(chunk).await.is_err() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
            let _ = sock.shutdown().await;
        }
    });

    // 2. Start Bonding Server accepting multiple subflows
    let bond_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bond_addr = bond_listener.local_addr().unwrap();
    let psk = "integration-test-secret-psk";
    let sessions: Arc<Mutex<HashMap<[u8; 16], Arc<BondingSession>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let sessions_srv = sessions.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = bond_listener.accept().await {
            let s_map = sessions_srv.clone();
            tokio::spawn(async move {
                if let Ok((meta, sealer, opener)) = server_handshake(&mut sock, psk).await {
                    let sess = {
                        let mut m = s_map.lock().await;
                        m.entry(meta.session_id)
                            .or_insert_with(|| BondingSession::new(meta.session_id, true))
                            .clone()
                    };
                    sess.attach_subflow(
                        meta.subflow_id,
                        meta.interface_name,
                        sock,
                        sealer,
                        opener,
                    )
                    .await;
                }
            });
        }
    });

    // 3. Connect 3 distinct encrypted client subflows (Wi-Fi, Ethernet, USB Tethering)
    let session_id = [0x42u8; 16];
    let (frame_tx, mut frame_rx) = mpsc::channel::<(u32, BondingFrame)>(512);

    let mut subflow_writers = Vec::new();
    let mut subflow_readers = Vec::new();

    for (sf_id, name) in [(1u32, "Wi-Fi"), (2u32, "Ethernet"), (3u32, "USB-Tethering")] {
        let mut stream = TcpStream::connect(bond_addr).await.unwrap();
        let (sealer, mut opener) =
            client_handshake(&mut stream, session_id, sf_id, name.into(), psk)
                .await
                .unwrap();
        let (mut r_half, w_half) = stream.into_split();
        subflow_writers.push((sf_id, w_half, sealer));

        let tx = frame_tx.clone();
        let reader_handle = tokio::spawn(async move {
            while let Ok(frame) = read_encrypted_frame(&mut r_half, &mut opener).await {
                if tx.send((sf_id, frame)).await.is_err() {
                    break;
                }
            }
        });
        subflow_readers.push(reader_handle);
    }
    drop(frame_tx);

    // 4. Open relayed TCP stream to origin server via Subflow #1
    let (_, ref mut w1, ref mut s1) = subflow_writers[0];
    write_encrypted_frame(
        w1,
        s1,
        &BondingFrame::TcpOpen {
            stream_id: 777,
            target_host: "127.0.0.1".into(),
            target_port: origin_addr.port(),
        },
    )
    .await
    .unwrap();

    // Wait for TcpOpenAck
    let mut got_ack = false;
    while let Some((_, f)) = frame_rx.recv().await {
        if let BondingFrame::TcpOpenAck { stream_id: 777, ok, .. } = f {
            assert!(ok);
            got_ack = true;
            break;
        }
    }
    assert!(got_ack);

    // Send trigger request chunk via Subflow #3
    {
        let (_, ref mut w3, ref mut s3) = subflow_writers[2];
        write_encrypted_frame(
            w3,
            s3,
            &BondingFrame::StreamChunk {
                stream_id: 777,
                offset: 0,
                eof: false,
                data: b"GET\n".to_vec(),
            },
        )
        .await
        .unwrap();
    }

    // 5. Receive striped chunks across subflows AND abruptly kill Subflow #2 mid-download!
    let mut reassembler = StreamReassembler::new(8 * 1024 * 1024);
    let mut received = Vec::with_capacity(expected_payload.len());
    let mut subflows_seen = std::collections::HashSet::new();
    let mut killed_sf2 = false;

    while let Some((sf_id, frame)) = frame_rx.recv().await {
        subflows_seen.insert(sf_id);
        if let BondingFrame::StreamChunk {
            stream_id: 777,
            offset,
            eof,
            data,
        } = frame
        {
            let ordered = reassembler.push_chunk(offset, data, eof).unwrap();
            received.extend_from_slice(&ordered);

            // Simulate sudden network cable unplug on Subflow #2 at ~35% progress!
            if !killed_sf2 && received.len() > 512 * 1024 {
                killed_sf2 = true;
                subflow_readers[1].abort();
                let (_, mut dead_w, _) = subflow_writers.remove(1);
                let _ = dead_w.shutdown().await;
                drop(dead_w);
            }

            if reassembler.is_complete() {
                break;
            }
        }
    }

    // Verify multiple subflows contributed AND 100% byte-exact payload reassembly despite mid-stream disconnect!
    assert!(
        subflows_seen.len() >= 2,
        "Expected traffic striped across multiple subflows, saw {:?}",
        subflows_seen
    );
    assert_eq!(received.len(), expected_payload.len());
    assert_eq!(received, expected_payload);
}

