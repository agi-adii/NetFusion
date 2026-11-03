use bonding_server::session::{hex_short, BondingSession};
use netfusion_protocol::server_handshake;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut listen_addr =
        env::var("NETFUSION_LISTEN").unwrap_or_else(|_| "0.0.0.0:9443".to_string());
    let mut psk = env::var("NETFUSION_PSK")
        .unwrap_or_else(|_| "netfusion-default-change-me-in-prod".to_string());
    let mut allow_local_targets = env::var("NETFUSION_ALLOW_LOCAL")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" | "-l" if i + 1 < args.len() => {
                listen_addr = args[i + 1].clone();
                i += 2;
            }
            "--psk" | "-k" if i + 1 < args.len() => {
                psk = args[i + 1].clone();
                i += 2;
            }
            "--allow-local-targets" => {
                allow_local_targets = true;
                i += 1;
            }
            "--help" | "-h" => {
                println!(
                    "NetFusion Multipath Bonding Server v1.0.0\n\
                     Usage: netfusion-bonding-server [OPTIONS]\n\n\
                     Options:\n\
                       -l, --listen <HOST:PORT>     Bind address (default: 0.0.0.0:9443 or NETFUSION_LISTEN)\n\
                       -k, --psk <SECRET>           Pre-shared auth key (or NETFUSION_PSK env var)\n\
                       --allow-local-targets        Permit local/LAN relay targets (for local testing)\n"
                );
                return Ok(());
            }
            _ => {
                i += 1;
            }
        }
    }

    if psk == "netfusion-default-change-me-in-prod" {
        eprintln!(
            "[WARN] Running with default development PSK. Set NETFUSION_PSK or --psk <SECRET> on production VPS!"
        );
    }

    let listener = TcpListener::bind(&listen_addr).await?;
    println!("==============================================================");
    println!(" NetFusion Multipath Encrypted Bonding Server v1.0.0");
    println!(" Listening on      : {}", listen_addr);
    println!(" Encryption        : X25519 ECDH + HKDF-SHA256 + ChaCha20-Poly1305");
    println!(" Authentication    : Mutual HMAC-SHA256 Challenge (PSK Protected)");
    println!(" Local Target Mode : {}", allow_local_targets);
    println!("==============================================================");

    let sessions: Arc<Mutex<HashMap<[u8; 16], Arc<BondingSession>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let psk = Arc::new(psk);

    loop {
        let (mut socket, peer_addr) = listener.accept().await?;
        let sessions_ref = sessions.clone();
        let psk_ref = psk.clone();

        tokio::spawn(async move {
            let handshake_res = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                server_handshake(&mut socket, &psk_ref),
            )
            .await;

            match handshake_res {
                Ok(Ok((meta, sealer, opener))) => {
                    let session = {
                        let mut map = sessions_ref.lock().await;
                        map.entry(meta.session_id)
                            .or_insert_with(|| {
                                BondingSession::new(meta.session_id, allow_local_targets)
                            })
                            .clone()
                    };

                    session
                        .clone()
                        .attach_subflow(
                            meta.subflow_id,
                            format!("{}@{}", meta.interface_name, peer_addr.ip()),
                            socket,
                            sealer,
                            opener,
                        )
                        .await;

                    // Cleanup session entry if all subflows disconnected
                    if session.active_subflow_count().await == 0 {
                        sessions_ref.lock().await.remove(&meta.session_id);
                        println!(
                            "[SESSION {}] Closed (0 active subflows)",
                            hex_short(&meta.session_id)
                        );
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("[AUTH REJECT] Peer {}: {}", peer_addr, e);
                }
                Err(_) => {
                    eprintln!("[TIMEOUT] Handshake timed out from {}", peer_addr);
                }
            }
        });
    }
}
