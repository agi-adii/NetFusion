use crate::bonding::client::BondingTunnelClient;
use crate::db::Database;
use crate::downloader::engine::build_http_client;
use crate::logger::AppLogger;
use crate::models::{
    now_ms, InterfaceStatus, NetworkInterfaceInfo, SpeedTestProgress, SpeedTestRecord,
};
use crate::network::monitor::NetworkMonitor;
use crate::network::socket_bind::probe_interface_rtt_ms;
use futures_util::StreamExt;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::task::JoinSet;

pub struct SpeedTestEngine {
    db: Arc<Database>,
    logger: AppLogger,
    monitor: Arc<NetworkMonitor>,
    bonding_client: Arc<BondingTunnelClient>,
    cancel_flag: Arc<AtomicBool>,
    running: AtomicBool,
    app_handle: Mutex<Option<AppHandle>>,
}

impl SpeedTestEngine {
    pub fn new(
        db: Arc<Database>,
        logger: AppLogger,
        monitor: Arc<NetworkMonitor>,
        bonding_client: Arc<BondingTunnelClient>,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            logger,
            monitor,
            bonding_client,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            running: AtomicBool::new(false),
            app_handle: Mutex::new(None),
        })
    }

    pub fn attach_app_handle(&self, handle: AppHandle) {
        if let Ok(mut g) = self.app_handle.lock() {
            *g = Some(handle);
        }
    }

    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
    }

    fn emit_progress(&self, prog: &SpeedTestProgress) {
        if let Ok(g) = self.app_handle.lock() {
            if let Some(ref app) = *g {
                let _ = app.emit("speedtest-progress", prog);
            }
        }
    }

    pub async fn run_test(&self, interface_id: Option<String>) -> Result<SpeedTestRecord, String> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err("A speed test is already in progress".into());
        }
        self.cancel_flag.store(false, Ordering::Relaxed);

        let res = self.execute_test_inner(interface_id).await;
        self.running.store(false, Ordering::SeqCst);
        res
    }

    async fn execute_test_inner(
        &self,
        interface_id: Option<String>,
    ) -> Result<SpeedTestRecord, String> {
        let snap = self.monitor.get_snapshot().await;
        let settings = self.db.get_settings();
        let base_endpoint = settings
            .speedtest_endpoint
            .trim_end_matches('/')
            .to_string();

        // Select target interfaces
        let (mode_label, target_label, target_nics): (String, String, Vec<NetworkInterfaceInfo>) =
            if let Some(ref if_id) = interface_id {
                let nic = snap
                    .interfaces
                    .into_iter()
                    .find(|i| &i.id == if_id)
                    .ok_or_else(|| "Selected network interface not found".to_string())?;
                if nic.status == InterfaceStatus::Disconnected || nic.ipv4.is_none() {
                    return Err(format!("Interface '{}' is disconnected", nic.name));
                }
                let lbl = nic.name.clone();
                ("Single Interface".into(), lbl, vec![nic])
            } else {
                let active: Vec<NetworkInterfaceInfo> = snap
                    .interfaces
                    .into_iter()
                    .filter(|i| {
                        i.enabled_for_bonding
                            && i.ipv4.is_some()
                            && matches!(
                                i.status,
                                InterfaceStatus::Connected | InterfaceStatus::Unstable
                            )
                    })
                    .collect();
                if active.is_empty() {
                    return Err("No enabled connected network interfaces available".into());
                }
                let names = active
                    .iter()
                    .map(|n| n.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" + ");
                let mode = if self.bonding_client.is_tunnel_connected() {
                    "Bonded Tunnel"
                } else {
                    "Aggregate Multi-Path"
                };
                (mode.into(), names, active)
            };

        self.logger.info(
            "SPEEDTEST",
            format!("Starting {mode_label} speed test over [{target_label}]"),
        );

        // PHASE 1: Real Ping, Jitter & Packet Loss Measurement (10 probes)
        let mut rtt_samples = Vec::new();
        let total_probes = 10usize;
        let mut lost_probes = 0usize;

        for p in 0..total_probes {
            if self.cancel_flag.load(Ordering::Relaxed) {
                return Err("Speed test cancelled".into());
            }
            let nic = &target_nics[p % target_nics.len()];
            let ip4 = nic
                .ipv4
                .as_deref()
                .and_then(|s| s.parse::<Ipv4Addr>().ok())
                .unwrap_or(Ipv4Addr::UNSPECIFIED);

            if let Some(ms) = probe_interface_rtt_ms(ip4, nic.if_index).await {
                rtt_samples.push(ms);
            } else {
                lost_probes += 1;
            }

            let cur_ping = if !rtt_samples.is_empty() {
                Some(
                    rtt_samples
                        .iter()
                        .copied()
                        .fold(f64::INFINITY, f64::min),
                )
            } else {
                None
            };
            let cur_jitter = compute_jitter(&rtt_samples);
            let cur_loss = (lost_probes as f64 / (p + 1) as f64) * 100.0;

            self.emit_progress(&SpeedTestProgress {
                phase: "Ping".into(),
                progress_pct: ((p + 1) as f64 / total_probes as f64) * 20.0,
                current_bps: 0,
                ping_ms: cur_ping,
                jitter_ms: cur_jitter,
                packet_loss_pct: cur_loss,
                download_bps: 0,
                upload_bps: 0,
                target_label: target_label.clone(),
                message: format!("Measuring RTT latency & packet loss ({}/{})...", p + 1, total_probes),
            });
        }

        let ping_ms = if !rtt_samples.is_empty() {
            rtt_samples.iter().copied().fold(f64::INFINITY, f64::min)
        } else {
            0.0
        };
        let jitter_ms = compute_jitter(&rtt_samples).unwrap_or(0.0);
        let packet_loss_pct = (lost_probes as f64 / total_probes as f64) * 100.0;

        // Build HTTP clients for test streams
        let use_tunnel = interface_id.is_none() && self.bonding_client.is_tunnel_connected();
        let clients: Vec<reqwest::Client> = if use_tunnel {
            let port = self.bonding_client.local_socks_port();
            vec![build_http_client(port)?]
        } else {
            let mut v = Vec::new();
            for nic in &target_nics {
                v.push(build_http_client(nic.local_proxy_port)?);
            }
            v
        };

        // PHASE 2: Real Multi-Stream Download Throughput Test (6.0 seconds)
        let dl_url = format!("{base_endpoint}/__down?bytes=25000000");
        let dl_bytes = Arc::new(AtomicU64::new(0));
        let dl_stop = Arc::new(AtomicBool::new(false));
        let dl_duration = Duration::from_secs(6);
        let dl_start = Instant::now();

        let mut dl_workers = JoinSet::new();
        // 3 streams per active NIC (or 6 streams over bonded tunnel)
        let streams_per_client = if use_tunnel { 6 } else { 3 };
        for client in &clients {
            for _ in 0..streams_per_client {
                let c = client.clone();
                let u = dl_url.clone();
                let counter = dl_bytes.clone();
                let stop = dl_stop.clone();
                let cancel = self.cancel_flag.clone();

                dl_workers.spawn(async move {
                    while !stop.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                        if let Ok(resp) = c.get(&u).send().await {
                            let mut stream = resp.bytes_stream();
                            while let Some(Ok(chunk)) = stream.next().await {
                                counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                                if stop.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed) {
                                    break;
                                }
                            }
                        } else {
                            tokio::time::sleep(Duration::from_millis(200)).await;
                        }
                    }
                });
            }
        }

        let mut prev_b = 0u64;
        let mut prev_t = Instant::now();
        let mut measured_dl_bps;

        while dl_start.elapsed() < dl_duration {
            if self.cancel_flag.load(Ordering::Relaxed) {
                dl_stop.store(true, Ordering::Relaxed);
                dl_workers.abort_all();
                return Err("Speed test cancelled".into());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
            let cur_b = dl_bytes.load(Ordering::Relaxed);
            let dt = prev_t.elapsed().as_secs_f64().max(0.05);
            prev_t = Instant::now();
            let inst_bps = (((cur_b.saturating_sub(prev_b)) as f64 * 8.0) / dt) as u64;
            prev_b = cur_b;

            let total_el = dl_start.elapsed().as_secs_f64().max(0.1);
            measured_dl_bps = ((cur_b as f64 * 8.0) / total_el) as u64;
            let pct = 20.0 + (total_el / dl_duration.as_secs_f64()).min(1.0) * 45.0;

            self.emit_progress(&SpeedTestProgress {
                phase: "Download".into(),
                progress_pct: pct,
                current_bps: inst_bps.max(measured_dl_bps),
                ping_ms: Some(ping_ms),
                jitter_ms: Some(jitter_ms),
                packet_loss_pct,
                download_bps: measured_dl_bps,
                upload_bps: 0,
                target_label: target_label.clone(),
                message: "Benchmarking multi-stream download bandwidth...".into(),
            });
        }
        dl_stop.store(true, Ordering::Relaxed);
        dl_workers.abort_all();

        let total_dl_bytes = dl_bytes.load(Ordering::Relaxed);
        // If external Cloudflare endpoint yielded 0 bytes while Bonding Tunnel is connected (e.g. local isolated Bonding Server test),
        // benchmark directly against the connected Bonding Server!
        let mut server_label = "Cloudflare Edge".to_string();
        let mut measured_ul_bps;

        if total_dl_bytes == 0 && use_tunnel {
            server_label = format!(
                "Bonding Server ({})",
                self.bonding_client
                    .snapshot(settings.bonding_mode)
                    .server_endpoint
            );
            let (tun_dl, tun_ul) = self
                .bonding_client
                .benchmark_tunnel_throughput(32 * 1024 * 1024, 16 * 1024 * 1024)
                .await?;
            measured_dl_bps = tun_dl;
            measured_ul_bps = tun_ul;
        } else {
            measured_dl_bps =
                ((total_dl_bytes as f64 * 8.0) / dl_start.elapsed().as_secs_f64().max(0.1)) as u64;

            // PHASE 3: Real Multi-Stream Upload Throughput Test (5.0 seconds)
            let ul_url = format!("{base_endpoint}/__up");
            let ul_bytes = Arc::new(AtomicU64::new(0));
            let ul_stop = Arc::new(AtomicBool::new(false));
            let ul_duration = Duration::from_secs(5);
            let ul_start = Instant::now();
            let payload_block = vec![0x42u8; 512 * 1024]; // 512 KiB upload POST blocks

            let mut ul_workers = JoinSet::new();
            let ul_streams = if use_tunnel { 4 } else { 2 };
            for client in &clients {
                for _ in 0..ul_streams {
                    let c = client.clone();
                    let u = ul_url.clone();
                    let block = payload_block.clone();
                    let counter = ul_bytes.clone();
                    let stop = ul_stop.clone();
                    let cancel = self.cancel_flag.clone();

                    ul_workers.spawn(async move {
                        while !stop.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                            let len = block.len() as u64;
                            if let Ok(resp) = c.post(&u).body(block.clone()).send().await {
                                if resp.status().is_success() {
                                    counter.fetch_add(len, Ordering::Relaxed);
                                }
                            } else {
                                tokio::time::sleep(Duration::from_millis(200)).await;
                            }
                        }
                    });
                }
            }

            let mut prev_ub = 0u64;
            let mut prev_ut = Instant::now();

            while ul_start.elapsed() < ul_duration {
                if self.cancel_flag.load(Ordering::Relaxed) {
                    ul_stop.store(true, Ordering::Relaxed);
                    ul_workers.abort_all();
                    return Err("Speed test cancelled".into());
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
                let cur_ub = ul_bytes.load(Ordering::Relaxed);
                let dt = prev_ut.elapsed().as_secs_f64().max(0.05);
                prev_ut = Instant::now();
                let inst_bps = (((cur_ub.saturating_sub(prev_ub)) as f64 * 8.0) / dt) as u64;
                prev_ub = cur_ub;

                let total_el = ul_start.elapsed().as_secs_f64().max(0.1);
                measured_ul_bps = ((cur_ub as f64 * 8.0) / total_el) as u64;
                let pct = 65.0 + (total_el / ul_duration.as_secs_f64()).min(1.0) * 35.0;

                self.emit_progress(&SpeedTestProgress {
                    phase: "Upload".into(),
                    progress_pct: pct,
                    current_bps: inst_bps.max(measured_ul_bps),
                    ping_ms: Some(ping_ms),
                    jitter_ms: Some(jitter_ms),
                    packet_loss_pct,
                    download_bps: measured_dl_bps,
                    upload_bps: measured_ul_bps,
                    target_label: target_label.clone(),
                    message: "Benchmarking multi-stream upload bandwidth...".into(),
                });
            }
            ul_stop.store(true, Ordering::Relaxed);
            ul_workers.abort_all();

            let total_ul_bytes = ul_bytes.load(Ordering::Relaxed);
            measured_ul_bps =
                ((total_ul_bytes as f64 * 8.0) / ul_start.elapsed().as_secs_f64().max(0.1)) as u64;
        }

        if measured_dl_bps == 0 && measured_ul_bps == 0 {
            return Err(
                "Speed test failed: could not transfer data over the selected network interface(s)"
                    .into(),
            );
        }

        let record = self.db.insert_speed_test(&SpeedTestRecord {
            id: 0,
            timestamp_ms: now_ms(),
            mode_label: mode_label.clone(),
            interface_name: target_label.clone(),
            server_label,
            download_bps: measured_dl_bps,
            upload_bps: measured_ul_bps,
            ping_ms,
            jitter_ms,
            packet_loss_pct,
        });

        self.emit_progress(&SpeedTestProgress {
            phase: "Complete".into(),
            progress_pct: 100.0,
            current_bps: measured_dl_bps,
            ping_ms: Some(ping_ms),
            jitter_ms: Some(jitter_ms),
            packet_loss_pct,
            download_bps: measured_dl_bps,
            upload_bps: measured_ul_bps,
            target_label,
            message: "Speed test completed".into(),
        });

        self.logger.success(
            "SPEEDTEST",
            format!(
                "{mode_label} Result: Down {:.2} Mbps, Up {:.2} Mbps, Ping {:.1} ms",
                measured_dl_bps as f64 / 1_000_000.0,
                measured_ul_bps as f64 / 1_000_000.0,
                ping_ms
            ),
        );

        Ok(record)
    }
}

fn compute_jitter(samples: &[f64]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let mut sum = 0.0;
    for w in samples.windows(2) {
        sum += (w[1] - w[0]).abs();
    }
    Some(sum / (samples.len() - 1) as f64)
}

