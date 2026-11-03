use crate::bonding::client::BondingTunnelClient;
use crate::db::Database;
use crate::logger::AppLogger;
use crate::models::{
    now_ms, InterfaceStatus, NetworkInterfaceInfo, NetworkTelemetrySnapshot,
};
use crate::network::socket_bind::{probe_interface_rtt_ms, spawn_interface_socks5_bridge};
use crate::network::win_api::enumerate_windows_adapters;
use chrono::Local;
use netfusion_protocol::scheduler::{compute_subflow_weights, BondingPolicy, SubflowDescriptor};
use std::collections::{HashMap, VecDeque};
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

struct NicRuntimeTracker {
    prev_rx_octets: u64,
    prev_tx_octets: u64,
    prev_sample_at: Instant,
    session_rx: u64,
    session_tx: u64,
    download_bps: u64,
    upload_bps: u64,
    ewma_download_bps: u64,
    rtt_history: VecDeque<Option<f64>>,
    last_status: Option<InterfaceStatus>,
    socks_bridge: Option<(Ipv4Addr, u16, JoinHandle<()>)>,
}

pub struct NetworkMonitor {
    db: Arc<Database>,
    logger: AppLogger,
    bonding_client: Arc<BondingTunnelClient>,
    latest_snapshot: RwLock<NetworkTelemetrySnapshot>,
    trackers: tokio::sync::Mutex<HashMap<String, NicRuntimeTracker>>,
    tick_counter: std::sync::atomic::AtomicU64,
}

impl NetworkMonitor {
    pub fn new(
        db: Arc<Database>,
        logger: AppLogger,
        bonding_client: Arc<BondingTunnelClient>,
    ) -> Arc<Self> {
        let settings = db.get_settings();
        let initial = NetworkTelemetrySnapshot {
            timestamp_ms: now_ms(),
            total_download_bps: 0,
            total_upload_bps: 0,
            total_link_capacity_bps: 0,
            active_connections_count: 0,
            average_latency_ms: None,
            overall_stability_pct: 100.0,
            downloaded_today_bytes: 0,
            uploaded_today_bytes: 0,
            interfaces: Vec::new(),
            bonding: bonding_client.snapshot(settings.bonding_mode),
        };

        Arc::new(Self {
            db,
            logger,
            bonding_client,
            latest_snapshot: RwLock::new(initial),
            trackers: tokio::sync::Mutex::new(HashMap::new()),
            tick_counter: std::sync::atomic::AtomicU64::new(0),
        })
    }

    pub async fn get_snapshot(&self) -> NetworkTelemetrySnapshot {
        self.latest_snapshot.read().await.clone()
    }

    /// Returns active enabled interfaces along with their local pinned SOCKS5 proxy port & weight.
    pub async fn active_bonding_interfaces(&self) -> Vec<NetworkInterfaceInfo> {
        let snap = self.latest_snapshot.read().await;
        snap.interfaces
            .iter()
            .filter(|i| {
                i.enabled_for_bonding
                    && i.ipv4.is_some()
                    && (i.status == InterfaceStatus::Connected
                        || i.status == InterfaceStatus::Unstable)
            })
            .cloned()
            .collect()
    }

    pub fn start_background_loop(self: Arc<Self>, app_handle: Option<AppHandle>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let snap = self.sample_once(&app_handle).await;
                if let Some(ref app) = app_handle {
                    let _ = app.emit("network-telemetry", &snap);
                }
            }
        });
    }

    pub async fn sample_once(
        &self,
        app_handle: &Option<AppHandle>,
    ) -> NetworkTelemetrySnapshot {
        let tick = self
            .tick_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let raw_adapters = tokio::task::spawn_blocking(enumerate_windows_adapters)
            .await
            .unwrap_or_default();

        let prefs = self.db.load_interface_prefs();
        let settings = self.db.get_settings();
        let policy: BondingPolicy = settings.bonding_mode;
        let day_key = Local::now().format("%Y-%m-%d").to_string();

        // Probe RTT every 3 seconds (plus tick 1 for immediate second jitter sample) asynchronously on connected IPv4 adapters
        let mut rtt_updates: HashMap<String, Option<f64>> = HashMap::new();
        if tick % 3 == 0 || tick == 1 {
            let mut probe_futs = Vec::new();
            for raw in &raw_adapters {
                if raw.is_up {
                    if let Some(ipv4) = raw.ipv4 {
                        let id = raw.id.clone();
                        let if_idx = raw.if_index;
                        probe_futs.push(async move {
                            let rtt = probe_interface_rtt_ms(ipv4, if_idx).await;
                            (id, rtt)
                        });
                    }
                }
            }
            for (id, rtt) in futures_util::future::join_all(probe_futs).await {
                rtt_updates.insert(id, rtt);
            }
        }

        let mut trackers = self.trackers.lock().await;
        let now = Instant::now();
        let mut interfaces = Vec::with_capacity(raw_adapters.len());

        // Determine if any primary failover interface is explicitly set
        let any_primary_set = prefs.values().any(|(_, _, p)| *p);

        for (idx, raw) in raw_adapters.iter().enumerate() {
            let (enabled, custom_weight, mut is_primary) = prefs
                .get(&raw.id)
                .copied()
                .unwrap_or((true, 50, false));
            if !any_primary_set && idx == 0 {
                is_primary = true;
            }

            let tracker = trackers.entry(raw.id.clone()).or_insert_with(|| NicRuntimeTracker {
                prev_rx_octets: raw.rx_octets,
                prev_tx_octets: raw.tx_octets,
                prev_sample_at: now,
                session_rx: 0,
                session_tx: 0,
                download_bps: 0,
                upload_bps: 0,
                ewma_download_bps: 0,
                rtt_history: VecDeque::with_capacity(12),
                last_status: None,
                socks_bridge: None,
            });

            let dt = now
                .duration_since(tracker.prev_sample_at)
                .as_secs_f64()
                .max(0.2);
            let rx_delta = raw.rx_octets.saturating_sub(tracker.prev_rx_octets);
            let tx_delta = raw.tx_octets.saturating_sub(tracker.prev_tx_octets);
            tracker.prev_rx_octets = raw.rx_octets;
            tracker.prev_tx_octets = raw.tx_octets;
            tracker.prev_sample_at = now;

            // Guard against initial wraparound spikes > 40 Gbps
            if rx_delta < 5_000_000_000 && tx_delta < 5_000_000_000 && tick > 0 {
                tracker.session_rx += rx_delta;
                tracker.session_tx += tx_delta;
                tracker.download_bps = ((rx_delta as f64 * 8.0) / dt) as u64;
                tracker.upload_bps = ((tx_delta as f64 * 8.0) / dt) as u64;
                tracker.ewma_download_bps = if tracker.ewma_download_bps == 0 {
                    tracker.download_bps
                } else {
                    ((tracker.ewma_download_bps as f64 * 0.65)
                        + (tracker.download_bps as f64 * 0.35)) as u64
                };
                self.db
                    .add_daily_usage(&day_key, &raw.id, rx_delta, tx_delta);
            }

            if let Some(probed) = rtt_updates.get(&raw.id) {
                if tracker.rtt_history.len() >= 10 {
                    tracker.rtt_history.pop_front();
                }
                tracker.rtt_history.push_back(*probed);
            }

            // Ensure per-interface IP_UNICAST_IF SOCKS5 bridge is running when adapter is up with IPv4
            if raw.is_up {
                if let Some(ip4) = raw.ipv4 {
                    let needs_respawn = match &tracker.socks_bridge {
                        Some((bound_ip, _, handle)) => *bound_ip != ip4 || handle.is_finished(),
                        None => true,
                    };
                    if needs_respawn {
                        if let Some((_, _, old_h)) = tracker.socks_bridge.take() {
                            old_h.abort();
                        }
                        if let Ok((port, handle)) =
                            spawn_interface_socks5_bridge(ip4, raw.if_index).await
                        {
                            tracker.socks_bridge = Some((ip4, port, handle));
                        }
                    }
                }
            } else if let Some((_, _, old_h)) = tracker.socks_bridge.take() {
                old_h.abort();
            }

            // Compute latency, jitter, packet loss & stability score from rolling RTT window
            let total_probes = tracker.rtt_history.len();
            let ok_rtts: Vec<f64> = tracker.rtt_history.iter().filter_map(|x| *x).collect();
            let latency_ms = if !ok_rtts.is_empty() {
                Some(ok_rtts.iter().sum::<f64>() / ok_rtts.len() as f64)
            } else {
                None
            };
            let jitter_ms = if ok_rtts.len() >= 2 {
                let mut diffs = 0.0;
                for w in ok_rtts.windows(2) {
                    diffs += (w[1] - w[0]).abs();
                }
                Some(diffs / (ok_rtts.len() - 1) as f64)
            } else {
                None
            };

            let packet_loss_pct = if total_probes > 0 && raw.is_up && raw.ipv4.is_some() {
                let lost = total_probes - ok_rtts.len();
                (lost as f64 / total_probes as f64) * 100.0
            } else {
                0.0
            };

            let mut stability_score: f64 = if !raw.is_up || raw.ipv4.is_none() {
                0.0
            } else {
                let mut s = 100.0 - packet_loss_pct * 1.4;
                if let Some(j) = jitter_ms {
                    s -= (j / 3.5).min(25.0);
                }
                if let Some(l) = latency_ms {
                    if l > 120.0 {
                        s -= ((l - 120.0) / 12.0).min(20.0);
                    }
                }
                s.clamp(5.0, 100.0)
            };

            let status = if !raw.is_up || raw.ipv4.is_none() {
                stability_score = 0.0;
                InterfaceStatus::Disconnected
            } else if !enabled {
                InterfaceStatus::Disabled
            } else if packet_loss_pct >= 30.0
                || jitter_ms.unwrap_or(0.0) > 85.0
                || stability_score < 58.0
            {
                InterfaceStatus::Unstable
            } else {
                InterfaceStatus::Connected
            };

            // Log & record state transitions (connect / disconnect / instability)
            if tracker.last_status != Some(status) {
                if let Some(prev) = tracker.last_status {
                    let event_label = match status {
                        InterfaceStatus::Connected => "CONNECTED",
                        InterfaceStatus::Disconnected => "DISCONNECTED",
                        InterfaceStatus::Unstable => "UNSTABLE",
                        InterfaceStatus::Disabled => "DISABLED",
                    };
                    let details = format!(
                        "Transitioned from {:?} to {:?} (IPv4: {})",
                        prev,
                        status,
                        raw.ipv4
                            .map(|ip| ip.to_string())
                            .unwrap_or_else(|| "none".into())
                    );
                    let ev = self.db.insert_connection_event(
                        &raw.id,
                        &raw.name,
                        raw.kind,
                        event_label,
                        &details,
                    );
                    self.logger.info(
                        "NETWORK",
                        format!("{}: {} — {}", raw.name, event_label, details),
                    );
                    if let Some(ref app) = app_handle {
                        let _ = app.emit("connection-event", &ev);
                    }
                }
                tracker.last_status = Some(status);
            }

            interfaces.push(NetworkInterfaceInfo {
                id: raw.id.clone(),
                if_index: raw.if_index,
                name: raw.name.clone(),
                description: raw.description.clone(),
                kind: raw.kind,
                ipv4: raw.ipv4.map(|v| v.to_string()),
                ipv6: raw.ipv6.map(|v| v.to_string()),
                gateway: raw.gateway.map(|v| v.to_string()),
                mac_address: raw.mac_address.clone(),
                status,
                enabled_for_bonding: enabled,
                is_primary_failover: is_primary,
                link_speed_bps: raw.link_speed_bps,
                download_bps: tracker.download_bps,
                upload_bps: tracker.upload_bps,
                latency_ms,
                jitter_ms,
                packet_loss_pct,
                stability_score,
                rx_bytes_session: tracker.session_rx,
                tx_bytes_session: tracker.session_tx,
                rx_bytes_today: 0,
                tx_bytes_today: 0,
                allocated_weight_pct: 0.0,
                custom_weight,
                subflow_connected: self.bonding_client.is_subflow_up(raw.if_index),
                local_proxy_port: tracker.socks_bridge.as_ref().map(|(_, p, _)| *p),
            });
        }
        drop(trackers);

        // Populate today's SQLite persisted byte totals
        let daily_map = self.db.load_daily_usage(&day_key);
        let mut downloaded_today_bytes = 0u64;
        let mut uploaded_today_bytes = 0u64;
        for nic in &mut interfaces {
            if let Some((rx_d, tx_d)) = daily_map.get(&nic.id) {
                nic.rx_bytes_today = *rx_d;
                nic.tx_bytes_today = *tx_d;
                downloaded_today_bytes += *rx_d;
                uploaded_today_bytes += *tx_d;
            }
        }

        // Compute dynamic multipath traffic allocation weights across enabled connected interfaces
        let descriptors: Vec<SubflowDescriptor> = interfaces
            .iter()
            .map(|nic| SubflowDescriptor {
                subflow_id: nic.if_index,
                throughput_bps: nic.download_bps,
                link_capacity_bps: nic.link_speed_bps,
                rtt_ms: nic.latency_ms.unwrap_or(28.0),
                loss_pct: nic.packet_loss_pct,
                is_primary: nic.is_primary_failover,
                custom_weight: nic.custom_weight,
                alive: nic.enabled_for_bonding
                    && nic.ipv4.is_some()
                    && matches!(
                        nic.status,
                        InterfaceStatus::Connected | InterfaceStatus::Unstable
                    ),
                inflight_bytes: 0,
            })
            .collect();

        let weights = compute_subflow_weights(policy, &descriptors);
        let weight_map: HashMap<u32, f64> = weights.into_iter().collect();
        for nic in &mut interfaces {
            nic.allocated_weight_pct = weight_map.get(&nic.if_index).copied().unwrap_or(0.0);
        }

        // Sync active interfaces with the Bonding Tunnel Client so new/recovered NICs automatically attach subflows!
        self.bonding_client
            .reconcile_subflows(&interfaces, policy)
            .await;

        let active_nics: Vec<&NetworkInterfaceInfo> = interfaces
            .iter()
            .filter(|i| {
                i.enabled_for_bonding
                    && i.ipv4.is_some()
                    && matches!(
                        i.status,
                        InterfaceStatus::Connected | InterfaceStatus::Unstable
                    )
            })
            .collect();

        let total_download_bps: u64 = active_nics.iter().map(|i| i.download_bps).sum();
        let total_upload_bps: u64 = active_nics.iter().map(|i| i.upload_bps).sum();
        let total_link_capacity_bps: u64 = active_nics.iter().map(|i| i.link_speed_bps).sum();
        let active_connections_count = active_nics.len();

        let rtts: Vec<f64> = active_nics.iter().filter_map(|i| i.latency_ms).collect();
        let average_latency_ms = if !rtts.is_empty() {
            Some(rtts.iter().sum::<f64>() / rtts.len() as f64)
        } else {
            None
        };

        let overall_stability_pct = if !active_nics.is_empty() {
            active_nics.iter().map(|i| i.stability_score).sum::<f64>() / active_nics.len() as f64
        } else {
            0.0
        };

        let snapshot = NetworkTelemetrySnapshot {
            timestamp_ms: now_ms(),
            total_download_bps,
            total_upload_bps,
            total_link_capacity_bps,
            active_connections_count,
            average_latency_ms,
            overall_stability_pct,
            downloaded_today_bytes,
            uploaded_today_bytes,
            interfaces,
            bonding: self.bonding_client.snapshot(policy),
        };

        *self.latest_snapshot.write().await = snapshot.clone();
        snapshot
    }
}

