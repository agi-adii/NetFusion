use netfusion_protocol::{
    read_encrypted_frame,
    scheduler::{BondingPolicy, MultipathScheduler, StreamReassembler, SubflowDescriptor},
    write_encrypted_frame, BondingFrame, CipherHalf, DEFAULT_CHUNK_SIZE,
};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{lookup_host, TcpStream};
use tokio::sync::{mpsc, Mutex};

struct SubflowEntry {
    descriptor: SubflowDescriptor,
    _interface_name: String,
    tx: mpsc::Sender<BondingFrame>,
}

struct UpstreamRelay {
    reassembler: StreamReassembler,
    outbound_tx: mpsc::Sender<Vec<u8>>,
}

struct SpeedTestUploadState {
    started_at: Instant,
    received_bytes: u64,
}

pub struct BondingSession {
    pub session_id: [u8; 16],
    allow_local_targets: bool,
    policy: Mutex<BondingPolicy>,
    scheduler: Mutex<MultipathScheduler>,
    subflows: Mutex<HashMap<u32, SubflowEntry>>,
    streams: Mutex<HashMap<u32, UpstreamRelay>>,
    speedtest_uploads: Mutex<HashMap<u32, SpeedTestUploadState>>,
}

impl BondingSession {
    pub fn new(session_id: [u8; 16], allow_local_targets: bool) -> Arc<Self> {
        Arc::new(Self {
            session_id,
            allow_local_targets,
            policy: Mutex::new(BondingPolicy::Smart),
            scheduler: Mutex::new(MultipathScheduler::new()),
            subflows: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
            speedtest_uploads: Mutex::new(HashMap::new()),
        })
    }

    pub async fn active_subflow_count(&self) -> usize {
        self.subflows.lock().await.len()
    }

    pub async fn attach_subflow(
        self: Arc<Self>,
        subflow_id: u32,
        interface_name: String,
        stream: TcpStream,
        mut sealer: CipherHalf,
        mut opener: CipherHalf,
    ) {
        let _ = stream.set_nodelay(true);
        let (mut reader, mut writer) = stream.into_split();
        let (tx, mut rx) = mpsc::channel::<BondingFrame>(256);

        {
            let mut map = self.subflows.lock().await;
            let is_first = map.is_empty();
            map.insert(
                subflow_id,
                SubflowEntry {
                    descriptor: SubflowDescriptor {
                        subflow_id,
                        throughput_bps: 25_000_000,
                        link_capacity_bps: 100_000_000,
                        rtt_ms: 25.0,
                        loss_pct: 0.0,
                        is_primary: is_first,
                        custom_weight: 50,
                        alive: true,
                        inflight_bytes: 0,
                    },
                    _interface_name: interface_name.clone(),
                    tx: tx.clone(),
                },
            );
        }

        println!(
            "[SESSION {}] Attached subflow #{} ({})",
            hex_short(&self.session_id),
            subflow_id,
            interface_name
        );

        let session_for_writer = self.clone();
        let writer_task = tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                if let Err(e) = write_encrypted_frame(&mut writer, &mut sealer, &frame).await {
                    eprintln!(
                        "[SESSION {}] Subflow #{} write error ({}); failing over frame",
                        hex_short(&session_for_writer.session_id),
                        subflow_id,
                        e
                    );
                    session_for_writer.remove_subflow(subflow_id).await;
                    // Automatic failover re-dispatch of dropped frame onto surviving subflows!
                    let _ = session_for_writer.send_multipath_frame(frame).await;
                    break;
                }
            }
        });

        while let Ok(frame) = read_encrypted_frame(&mut reader, &mut opener).await {
            self.clone().handle_incoming_frame(subflow_id, &tx, frame).await;
        }

        self.remove_subflow(subflow_id).await;
        writer_task.abort();
        println!(
            "[SESSION {}] Subflow #{} ({}) disconnected",
            hex_short(&self.session_id),
            subflow_id,
            interface_name
        );
    }

    async fn remove_subflow(&self, subflow_id: u32) {
        self.subflows.lock().await.remove(&subflow_id);
    }

    /// Schedules and sends a frame across the optimal active subflow according to the BondingPolicy.
    /// If the selected subflow channel closes mid-flight, automatically retries on remaining subflows.
    pub async fn send_multipath_frame(&self, mut frame: BondingFrame) -> Result<(), String> {
        let chunk_len = match &frame {
            BondingFrame::StreamChunk { data, .. } => data.len(),
            _ => 512,
        };

        for _attempt in 0..6 {
            let policy = *self.policy.lock().await;
            let chosen_tx = {
                let map = self.subflows.lock().await;
                if map.is_empty() {
                    return Err("No active subflows in session".into());
                }
                let descriptors: Vec<SubflowDescriptor> =
                    map.values().map(|e| e.descriptor.clone()).collect();
                let mut sched = self.scheduler.lock().await;
                let chosen_id = sched
                    .select_subflow(policy, &descriptors, chunk_len)
                    .or_else(|| descriptors.first().map(|d| d.subflow_id));
                chosen_id.and_then(|id| map.get(&id).map(|e| (id, e.tx.clone())))
            };

            if let Some((chosen_id, tx)) = chosen_tx {
                match tx.send(frame).await {
                    Ok(()) => return Ok(()),
                    Err(err) => {
                        frame = err.0;
                        self.remove_subflow(chosen_id).await;
                    }
                }
            } else {
                return Err("No eligible subflow available".into());
            }
        }
        Err("All subflows failed during frame dispatch".into())
    }

    async fn handle_incoming_frame(
        self: Arc<Self>,
        subflow_id: u32,
        direct_tx: &mpsc::Sender<BondingFrame>,
        frame: BondingFrame,
    ) {
        match frame {
            BondingFrame::Ping { seq, sent_ms } => {
                let _ = direct_tx.send(BondingFrame::Pong { seq, sent_ms }).await;
            }
            BondingFrame::Pong { .. } => {}
            BondingFrame::SetPolicy { policy } => {
                *self.policy.lock().await = policy;
            }
            BondingFrame::SubflowTelemetry {
                subflow_id: sf_id,
                throughput_bps,
                link_capacity_bps,
                rtt_ms,
                loss_permille,
                is_primary,
                custom_weight,
            } => {
                let mut map = self.subflows.lock().await;
                let key = if map.contains_key(&sf_id) {
                    sf_id
                } else {
                    subflow_id
                };
                if let Some(entry) = map.get_mut(&key) {
                    entry.descriptor.throughput_bps = throughput_bps;
                    entry.descriptor.link_capacity_bps = link_capacity_bps;
                    entry.descriptor.rtt_ms = (rtt_ms as f64).max(1.0);
                    entry.descriptor.loss_pct = (loss_permille as f64) / 10.0;
                    entry.descriptor.is_primary = is_primary;
                    entry.descriptor.custom_weight = custom_weight;
                }
            }
            BondingFrame::TcpOpen {
                stream_id,
                target_host,
                target_port,
            } => {
                let session = self.clone();
                tokio::spawn(async move {
                    session
                        .open_relayed_tcp_stream(stream_id, target_host, target_port)
                        .await;
                });
            }
            BondingFrame::StreamChunk {
                stream_id,
                offset,
                eof,
                data,
            } => {
                let mut ready_bytes = None;
                let mut completed = false;
                {
                    let mut streams = self.streams.lock().await;
                    if let Some(relay) = streams.get_mut(&stream_id) {
                        if let Ok(ordered) = relay.reassembler.push_chunk(offset, data, eof) {
                            if !ordered.is_empty() {
                                ready_bytes = Some((relay.outbound_tx.clone(), ordered));
                            }
                            completed = relay.reassembler.is_complete();
                        }
                    }
                }
                if let Some((tx, ordered)) = ready_bytes {
                    let _ = tx.send(ordered).await;
                }
                if completed {
                    self.streams.lock().await.remove(&stream_id);
                }
            }
            BondingFrame::StreamClose { stream_id, .. } => {
                self.streams.lock().await.remove(&stream_id);
            }
            BondingFrame::SpeedTestDownRequest {
                stream_id,
                total_bytes,
            } => {
                let session = self.clone();
                tokio::spawn(async move {
                    let capped = total_bytes.min(256 * 1024 * 1024); // Cap 256 MiB per test burst
                    let payload_template = vec![0xA5u8; DEFAULT_CHUNK_SIZE];
                    let mut offset = 0u64;
                    while offset < capped {
                        let rem = (capped - offset) as usize;
                        let take = rem.min(DEFAULT_CHUNK_SIZE);
                        let eof = offset + (take as u64) >= capped;
                        let chunk = BondingFrame::StreamChunk {
                            stream_id,
                            offset,
                            eof,
                            data: payload_template[..take].to_vec(),
                        };
                        if session.send_multipath_frame(chunk).await.is_err() {
                            break;
                        }
                        offset += take as u64;
                    }
                });
            }
            BondingFrame::SpeedTestUpChunk {
                stream_id,
                eof,
                data,
                ..
            } => {
                let mut finished = None;
                {
                    let mut uploads = self.speedtest_uploads.lock().await;
                    let entry = uploads.entry(stream_id).or_insert_with(|| SpeedTestUploadState {
                        started_at: Instant::now(),
                        received_bytes: 0,
                    });
                    entry.received_bytes += data.len() as u64;
                    if eof {
                        let elapsed_ms = entry.started_at.elapsed().as_millis().max(1) as u64;
                        finished = Some((entry.received_bytes, elapsed_ms));
                        uploads.remove(&stream_id);
                    }
                }
                if let Some((total_bytes, elapsed_ms)) = finished {
                    let _ = self
                        .send_multipath_frame(BondingFrame::SpeedTestUpResult {
                            stream_id,
                            total_bytes,
                            elapsed_ms,
                        })
                        .await;
                }
            }
            _ => {}
        }
    }

    async fn open_relayed_tcp_stream(
        self: Arc<Self>,
        stream_id: u32,
        target_host: String,
        target_port: u16,
    ) {
        let resolved = match resolve_and_validate_target(
            &target_host,
            target_port,
            self.allow_local_targets,
        )
        .await
        {
            Ok(addr) => addr,
            Err(err_msg) => {
                let _ = self
                    .send_multipath_frame(BondingFrame::TcpOpenAck {
                        stream_id,
                        ok: false,
                        error: Some(err_msg),
                    })
                    .await;
                return;
            }
        };

        let outbound = match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            TcpStream::connect(resolved),
        )
        .await
        {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                let _ = self
                    .send_multipath_frame(BondingFrame::TcpOpenAck {
                        stream_id,
                        ok: false,
                        error: Some(e.to_string()),
                    })
                    .await;
                return;
            }
            Err(_) => {
                let _ = self
                    .send_multipath_frame(BondingFrame::TcpOpenAck {
                        stream_id,
                        ok: false,
                        error: Some("Target connection timed out".into()),
                    })
                    .await;
                return;
            }
        };

        let _ = outbound.set_nodelay(true);
        let (mut out_read, mut out_write) = outbound.into_split();
        let (up_tx, mut up_rx) = mpsc::channel::<Vec<u8>>(256);

        self.streams.lock().await.insert(
            stream_id,
            UpstreamRelay {
                reassembler: StreamReassembler::new(32 * 1024 * 1024),
                outbound_tx: up_tx,
            },
        );

        let _ = self
            .send_multipath_frame(BondingFrame::TcpOpenAck {
                stream_id,
                ok: true,
                error: None,
            })
            .await;

        // Upstream writer: ordered reassembled chunks -> target remote server
        tokio::spawn(async move {
            while let Some(buf) = up_rx.recv().await {
                if out_write.write_all(&buf).await.is_err() {
                    break;
                }
            }
            let _ = out_write.shutdown().await;
        });

        // Downstream reader: target remote server -> multipath striped StreamChunks across ALL client NICs!
        let session_down = self.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; DEFAULT_CHUNK_SIZE];
            let mut offset = 0u64;
            loop {
                match out_read.read(&mut buf).await {
                    Ok(0) => {
                        let _ = session_down
                            .send_multipath_frame(BondingFrame::StreamChunk {
                                stream_id,
                                offset,
                                eof: true,
                                data: Vec::new(),
                            })
                            .await;
                        break;
                    }
                    Ok(n) => {
                        let frame = BondingFrame::StreamChunk {
                            stream_id,
                            offset,
                            eof: false,
                            data: buf[..n].to_vec(),
                        };
                        offset += n as u64;
                        if session_down.send_multipath_frame(frame).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = session_down
                            .send_multipath_frame(BondingFrame::StreamClose {
                                stream_id,
                                reason: e.to_string(),
                            })
                            .await;
                        break;
                    }
                }
            }
            session_down.streams.lock().await.remove(&stream_id);
        });
    }
}

async fn resolve_and_validate_target(
    host: &str,
    port: u16,
    allow_local: bool,
) -> Result<SocketAddr, String> {
    let addrs: Vec<SocketAddr> = lookup_host((host, port))
        .await
        .map_err(|e| format!("DNS resolution failed for {host}: {e}"))?
        .collect();

    for addr in addrs {
        if allow_local || is_public_routable_ip(addr.ip()) {
            return Ok(addr);
        }
    }
    Err(format!(
        "Target {host}:{port} blocked by SSRF security policy (private/loopback address)"
    ))
}

fn is_public_routable_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0b1100_0000) == 64)
        }
        IpAddr::V6(v6) => !(v6.is_loopback() || v6.is_unspecified()),
    }
}

pub fn hex_short(id: &[u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}",
        id[0], id[1], id[2], id[3]
    )
}

