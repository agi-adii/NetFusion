use crate::bonding::virtual_adapter::{
    disable_windows_system_proxy, enable_windows_system_proxy, is_windows_system_proxy_enabled,
};
use crate::db::Database;
use crate::logger::AppLogger;
use crate::models::{
    now_ms, BondingStateSnapshot, InterfaceStatus, NetworkInterfaceInfo, TunnelStatus,
};
use crate::network::socket_bind::resolve_and_connect_on_interface;
use netfusion_protocol::{
    client_handshake, read_encrypted_frame,
    scheduler::{BondingPolicy, MultipathScheduler, StreamReassembler, SubflowDescriptor},
    write_encrypted_frame, BondingFrame, ProtocolError, DEFAULT_CHUNK_SIZE,
};
use rand::RngCore;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};

struct ClientSubflow {
    #[allow(dead_code)]
    if_index: u32,
    interface_name: String,
    descriptor: SubflowDescriptor,
    tx: mpsc::Sender<BondingFrame>,
    abort_handle: tokio::task::AbortHandle,
}

struct RelayedStream {
    reassembler: StreamReassembler,
    down_tx: mpsc::Sender<Vec<u8>>,
}

pub struct BondingTunnelClient {
    db: Arc<Database>,
    logger: AppLogger,
    bonding_active: AtomicBool,
    auto_bonding: AtomicBool,
    adaptive_load_balancing: AtomicBool,
    system_proxy_routed: AtomicBool,
    virtual_adapter_detected: StdMutex<Option<String>>,
    direct_interfaces: StdMutex<Vec<(u32, Ipv4Addr, String, f64)>>,
    direct_rr_cursor: AtomicUsize,
    enabled: AtomicBool,
    status: StdMutex<TunnelStatus>,
    status_message: StdMutex<String>,
    endpoint: StdMutex<String>,
    session_id: StdMutex<[u8; 16]>,
    active_subflow_ids: StdMutex<Vec<u32>>,
    subflows: Mutex<HashMap<u32, ClientSubflow>>,
    connecting_subflows: StdMutex<Vec<u32>>,
    scheduler: Mutex<MultipathScheduler>,
    policy: StdMutex<BondingPolicy>,
    next_stream_id: AtomicU32,
    pending_opens: Mutex<HashMap<u32, oneshot::Sender<Result<(), String>>>>,
    streams: Mutex<HashMap<u32, RelayedStream>>,
    pending_speedtest_up: Mutex<HashMap<u32, oneshot::Sender<(u64, u64)>>>,
    total_rx_bytes: AtomicU64,
    total_tx_bytes: AtomicU64,
    last_rtt_us: AtomicU64,
    reassembly_depth: AtomicUsize,
    socks_port: AtomicU32,
    socks_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl BondingTunnelClient {
    pub fn new(db: Arc<Database>, logger: AppLogger) -> Arc<Self> {
        let settings = db.get_settings();
        let mut sid = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut sid);
        let sys_proxy = is_windows_system_proxy_enabled(settings.local_socks_port);

        Arc::new(Self {
            db,
            logger,
            bonding_active: AtomicBool::new(true),
            auto_bonding: AtomicBool::new(true),
            adaptive_load_balancing: AtomicBool::new(settings.adaptive_concurrency),
            system_proxy_routed: AtomicBool::new(sys_proxy),
            virtual_adapter_detected: StdMutex::new(None),
            direct_interfaces: StdMutex::new(Vec::new()),
            direct_rr_cursor: AtomicUsize::new(0),
            enabled: AtomicBool::new(settings.bonding_tunnel_enabled),
            status: StdMutex::new(TunnelStatus::Disconnected),
            status_message: StdMutex::new(
                "Active — Direct Multi-Path Bonding Gateway Ready".into(),
            ),
            endpoint: StdMutex::new(settings.bonding_server_endpoint),
            session_id: StdMutex::new(sid),
            active_subflow_ids: StdMutex::new(Vec::new()),
            subflows: Mutex::new(HashMap::new()),
            connecting_subflows: StdMutex::new(Vec::new()),
            scheduler: Mutex::new(MultipathScheduler::new()),
            policy: StdMutex::new(settings.bonding_mode),
            next_stream_id: AtomicU32::new(100),
            pending_opens: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
            pending_speedtest_up: Mutex::new(HashMap::new()),
            total_rx_bytes: AtomicU64::new(0),
            total_tx_bytes: AtomicU64::new(0),
            last_rtt_us: AtomicU64::new(0),
            reassembly_depth: AtomicUsize::new(0),
            socks_port: AtomicU32::new(0),
            socks_task: Mutex::new(None),
        })
    }

    pub fn is_bonding_active(&self) -> bool {
        self.bonding_active.load(Ordering::Relaxed)
    }

    pub async fn set_bonding_active(self: &Arc<Self>, active: bool) {
        self.bonding_active.store(active, Ordering::Relaxed);
        if active {
            let settings = self.db.get_settings();
            let _ = self
                .ensure_local_socks_listener(settings.local_socks_port)
                .await;
            *self.status_message.lock().unwrap() = if self.is_tunnel_connected() {
                format!(
                    "Encrypted Multipath Tunnel Connected ({} Subflows Active)",
                    self.active_subflow_ids.lock().unwrap().len()
                )
            } else {
                "Active — Direct Multi-Path Bonding Gateway Ready".into()
            };
            self.logger
                .success("BONDING", "Started Multi-Network Internet Bonding Engine");
        } else {
            if self.system_proxy_routed.swap(false, Ordering::Relaxed) {
                let _ = disable_windows_system_proxy();
            }
            self.disconnect_all_subflows().await;
            *self.status.lock().unwrap() = TunnelStatus::Disconnected;
            *self.status_message.lock().unwrap() =
                "Bonding Paused — Standard Single-Interface OS Routing".into();
            self.logger
                .info("BONDING", "Stopped Multi-Network Internet Bonding Engine");
        }
    }

    pub fn is_auto_bonding(&self) -> bool {
        self.auto_bonding.load(Ordering::Relaxed)
    }

    pub fn set_auto_bonding(&self, enabled: bool) {
        self.auto_bonding.store(enabled, Ordering::Relaxed);
        self.logger.info(
            "BONDING",
            format!(
                "Auto-Bonding {} for newly connected interfaces",
                if enabled { "enabled" } else { "disabled" }
            ),
        );
    }

    pub fn is_adaptive_load_balancing(&self) -> bool {
        self.adaptive_load_balancing.load(Ordering::Relaxed)
    }

    pub fn set_adaptive_load_balancing(&self, enabled: bool) {
        self.adaptive_load_balancing
            .store(enabled, Ordering::Relaxed);
        self.logger.info(
            "BONDING",
            format!(
                "Adaptive Load Balancing {}",
                if enabled { "enabled" } else { "disabled" }
            ),
        );
    }

    pub async fn set_system_proxy_routing(
        self: &Arc<Self>,
        enabled: bool,
    ) -> Result<(), String> {
        let settings = self.db.get_settings();
        let port = self
            .ensure_local_socks_listener(settings.local_socks_port)
            .await;
        if port == 0 {
            return Err("Failed to bind local Virtual Bonding Gateway port".into());
        }
        if enabled {
            enable_windows_system_proxy(port)?;
            self.system_proxy_routed.store(true, Ordering::Relaxed);
            self.logger.success(
                "BONDING",
                format!(
                    "Enabled Windows Application Traffic Routing via Virtual Bonding Gateway (127.0.0.1:{port})"
                ),
            );
        } else {
            disable_windows_system_proxy()?;
            self.system_proxy_routed.store(false, Ordering::Relaxed);
            self.logger.info(
                "BONDING",
                "Disabled Windows Application Traffic Routing (restored direct OS proxy settings)",
            );
        }
        Ok(())
    }

    pub fn update_direct_interfaces(
        &self,
        interfaces: &[NetworkInterfaceInfo],
        virtual_adapter: Option<String>,
    ) {
        *self.virtual_adapter_detected.lock().unwrap() = virtual_adapter;
        let mut ring = Vec::new();
        for nic in interfaces {
            if nic.enabled_for_bonding
                && matches!(
                    nic.status,
                    InterfaceStatus::Connected | InterfaceStatus::Unstable
                )
            {
                if let Some(ref ip_str) = nic.ipv4 {
                    if let Ok(ip4) = ip_str.parse::<Ipv4Addr>() {
                        let slots = ((nic.allocated_weight_pct / 10.0).round() as usize).clamp(1, 10);
                        for _ in 0..slots {
                            ring.push((
                                nic.if_index,
                                ip4,
                                nic.name.clone(),
                                nic.allocated_weight_pct,
                            ));
                        }
                    }
                }
            }
        }
        *self.direct_interfaces.lock().unwrap() = ring;
    }

    pub async fn ensure_local_socks_listener(self: &Arc<Self>, preferred_port: u16) -> u16 {
        let current = self.socks_port.load(Ordering::Relaxed) as u16;
        if current != 0 {
            return current;
        }
        let listener = match TcpListener::bind(("127.0.0.1", preferred_port)).await {
            Ok(l) => l,
            Err(_) => match TcpListener::bind("127.0.0.1:0").await {
                Ok(l) => l,
                Err(_) => return 0,
            },
        };
        let bound_port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        self.socks_port.store(bound_port as u32, Ordering::Relaxed);

        let client_ref = self.clone();
        let handle = tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let c = client_ref.clone();
                tokio::spawn(async move {
                    let _ = c.handle_local_socks5_stream(&mut sock).await;
                });
            }
        });
        *self.socks_task.lock().await = Some(handle);
        bound_port
    }

    pub fn is_tunnel_connected(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
            && *self.status.lock().unwrap() == TunnelStatus::Connected
            && !self.active_subflow_ids.lock().unwrap().is_empty()
    }

    pub fn local_socks_port(&self) -> Option<u16> {
        let p = self.socks_port.load(Ordering::Relaxed) as u16;
        if p > 0 {
            Some(p)
        } else {
            None
        }
    }

    pub fn is_subflow_up(&self, if_index: u32) -> bool {
        self.active_subflow_ids.lock().unwrap().contains(&if_index)
    }

    pub fn snapshot(&self, mode: BondingPolicy) -> BondingStateSnapshot {
        let active_subflows = self.active_subflow_ids.lock().unwrap().len();
        let rtt_us = self.last_rtt_us.load(Ordering::Relaxed);
        let socks_port = self.local_socks_port();
        BondingStateSnapshot {
            bonding_active: self.bonding_active.load(Ordering::Relaxed),
            auto_bonding: self.auto_bonding.load(Ordering::Relaxed),
            adaptive_load_balancing: self.adaptive_load_balancing.load(Ordering::Relaxed),
            virtual_gateway_active: socks_port.is_some()
                && self.bonding_active.load(Ordering::Relaxed),
            system_proxy_routed: self.system_proxy_routed.load(Ordering::Relaxed),
            virtual_adapter_detected: self.virtual_adapter_detected.lock().unwrap().clone(),
            mode,
            tunnel_enabled: self.enabled.load(Ordering::Relaxed),
            tunnel_status: *self.status.lock().unwrap(),
            server_endpoint: self.endpoint.lock().unwrap().clone(),
            active_subflows,
            local_socks_port: socks_port,
            tunnel_rtt_ms: if rtt_us > 0 {
                Some(rtt_us as f64 / 1000.0)
            } else {
                None
            },
            reassembly_queue_depth: self.reassembly_depth.load(Ordering::Relaxed),
            total_bonded_rx_bytes: self.total_rx_bytes.load(Ordering::Relaxed),
            total_bonded_tx_bytes: self.total_tx_bytes.load(Ordering::Relaxed),
            status_message: self.status_message.lock().unwrap().clone(),
        }
    }

    pub async fn connect_server(
        self: &Arc<Self>,
        endpoint: String,
        new_psk: Option<String>,
        interfaces: &[NetworkInterfaceInfo],
    ) -> Result<(), String> {
        if endpoint.trim().is_empty() || !endpoint.contains(':') {
            return Err("Endpoint must be formatted as host:port (e.g. 127.0.0.1:9443)".into());
        }
        if let Some(psk) = new_psk {
            if !psk.trim().is_empty() {
                self.db.store_bonding_psk(psk.trim())?;
            }
        }

        // Rotate fresh session_id for new tunnel connection
        let mut sid = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut sid);
        *self.session_id.lock().unwrap() = sid;
        *self.endpoint.lock().unwrap() = endpoint.trim().to_string();
        self.enabled.store(true, Ordering::Relaxed);
        *self.status.lock().unwrap() = TunnelStatus::Connecting;
        *self.status_message.lock().unwrap() =
            format!("Connecting encrypted multipath subflows to {}...", endpoint);

        let settings = self.db.get_settings();
        let _ = self
            .ensure_local_socks_listener(settings.local_socks_port)
            .await;

        self.disconnect_all_subflows().await;
        self.reconcile_subflows(interfaces, settings.bonding_mode)
            .await;

        // Give initial subflows up to 2.5s to complete handshake so UI gets immediate feedback
        for _ in 0..25 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let st = *self.status.lock().unwrap();
            if st == TunnelStatus::Connected {
                return Ok(());
            }
            if st == TunnelStatus::AuthFailed {
                return Err("Authentication rejected by Bonding Server: check your Pre-Shared Key".into());
            }
            if st == TunnelStatus::ServerUnreachable
                && self.connecting_subflows.lock().unwrap().is_empty()
            {
                return Err(format!(
                    "Bonding Server at {} is unreachable. Falling back to Direct Multi-Path HTTP Mode.",
                    endpoint
                ));
            }
        }
        Ok(())
    }

    pub async fn disconnect_server(&self) {
        self.enabled.store(false, Ordering::Relaxed);
        self.disconnect_all_subflows().await;
        *self.status.lock().unwrap() = TunnelStatus::Disconnected;
        *self.status_message.lock().unwrap() =
            "Disconnected — Direct Multi-Path HTTP Mode active".into();
        self.logger
            .info("BONDING", "Disconnected from Multipath Bonding Server");
    }

    async fn disconnect_all_subflows(&self) {
        let mut map = self.subflows.lock().await;
        for (_, sf) in map.drain() {
            sf.abort_handle.abort();
        }
        self.active_subflow_ids.lock().unwrap().clear();
    }

    /// Called every 1s by NetworkMonitor to keep encrypted subflows synced with enabled Windows NICs.
    pub async fn reconcile_subflows(
        self: &Arc<Self>,
        interfaces: &[NetworkInterfaceInfo],
        policy: BondingPolicy,
    ) {
        *self.policy.lock().unwrap() = policy;
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let desired: HashMap<u32, &NetworkInterfaceInfo> = interfaces
            .iter()
            .filter(|i| {
                i.enabled_for_bonding
                    && i.ipv4.is_some()
                    && matches!(
                        i.status,
                        InterfaceStatus::Connected | InterfaceStatus::Unstable
                    )
            })
            .map(|i| (i.if_index, i))
            .collect();

        // Remove subflows for disabled or disconnected adapters
        {
            let mut map = self.subflows.lock().await;
            let stale: Vec<u32> = map
                .keys()
                .copied()
                .filter(|id| !desired.contains_key(id))
                .collect();
            for id in stale {
                if let Some(sf) = map.remove(&id) {
                    sf.abort_handle.abort();
                    self.logger.warn(
                        "BONDING",
                        format!(
                            "Detached subflow on {} (interface disabled or disconnected)",
                            sf.interface_name
                        ),
                    );
                }
            }

            // Update descriptors & push live telemetry to server
            for (id, sf) in map.iter_mut() {
                if let Some(nic) = desired.get(id) {
                    sf.descriptor.throughput_bps = nic.download_bps;
                    sf.descriptor.link_capacity_bps = nic.link_speed_bps;
                    sf.descriptor.rtt_ms = nic.latency_ms.unwrap_or(25.0);
                    sf.descriptor.loss_pct = nic.packet_loss_pct;
                    sf.descriptor.is_primary = nic.is_primary_failover;
                    sf.descriptor.custom_weight = nic.custom_weight;

                    let _ = sf.tx.try_send(BondingFrame::SubflowTelemetry {
                        subflow_id: *id,
                        throughput_bps: nic.download_bps,
                        link_capacity_bps: nic.link_speed_bps,
                        rtt_ms: nic.latency_ms.unwrap_or(25.0) as u32,
                        loss_permille: (nic.packet_loss_pct * 10.0) as u16,
                        is_primary: nic.is_primary_failover,
                        custom_weight: nic.custom_weight,
                    });
                    let _ = sf.tx.try_send(BondingFrame::Ping {
                        seq: now_ms(),
                        sent_ms: now_ms(),
                    });
                }
            }
            if let Some((_, first_sf)) = map.iter().next() {
                let _ = first_sf.tx.try_send(BondingFrame::SetPolicy { policy });
            }

            *self.active_subflow_ids.lock().unwrap() = map.keys().copied().collect();
        }

        // Spawn subflow connection attempts for newly available interfaces
        let endpoint = self.endpoint.lock().unwrap().clone();
        let psk = self.db.load_bonding_psk();
        let session_id = *self.session_id.lock().unwrap();

        for (if_index, nic) in desired {
            let already_up = self.active_subflow_ids.lock().unwrap().contains(&if_index);
            let already_connecting = self
                .connecting_subflows
                .lock()
                .unwrap()
                .contains(&if_index);
            if already_up || already_connecting {
                continue;
            }
            let Some(ip_str) = nic.ipv4.clone() else {
                continue;
            };
            let Ok(local_ipv4) = ip_str.parse::<Ipv4Addr>() else {
                continue;
            };

            self.connecting_subflows.lock().unwrap().push(if_index);
            let client_ref = self.clone();
            let ep_clone = endpoint.clone();
            let psk_clone = psk.clone();
            let nic_clone = nic.clone();

            tokio::spawn(async move {
                client_ref
                    .establish_one_subflow(
                        session_id,
                        &ep_clone,
                        &psk_clone,
                        nic_clone,
                        local_ipv4,
                    )
                    .await;
                client_ref
                    .connecting_subflows
                    .lock()
                    .unwrap()
                    .retain(|x| *x != if_index);
            });
        }
    }

    async fn establish_one_subflow(
        self: &Arc<Self>,
        session_id: [u8; 16],
        endpoint: &str,
        psk: &str,
        nic: NetworkInterfaceInfo,
        local_ipv4: Ipv4Addr,
    ) {
        let Some((host, port_str)) = endpoint.rsplit_once(':') else {
            return;
        };
        let Ok(port) = port_str.parse::<u16>() else {
            return;
        };

        let mut tcp = match resolve_and_connect_on_interface(
            host,
            port,
            local_ipv4,
            nic.if_index,
            Duration::from_secs(5),
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                if self.active_subflow_ids.lock().unwrap().is_empty() {
                    *self.status.lock().unwrap() = TunnelStatus::ServerUnreachable;
                    *self.status_message.lock().unwrap() =
                        format!("Server {} unreachable ({})", endpoint, e);
                }
                return;
            }
        };

        let _ = tcp.set_nodelay(true);
        let hs = tokio::time::timeout(
            Duration::from_secs(6),
            client_handshake(
                &mut tcp,
                session_id,
                nic.if_index,
                nic.name.clone(),
                psk,
            ),
        )
        .await;

        let (mut sealer, mut opener) = match hs {
            Ok(Ok(pair)) => pair,
            Ok(Err(ProtocolError::AuthenticationFailed)) => {
                *self.status.lock().unwrap() = TunnelStatus::AuthFailed;
                *self.status_message.lock().unwrap() =
                    "Bonding Server authentication failed (invalid Pre-Shared Key)".into();
                self.logger.error(
                    "BONDING",
                    format!("Subflow auth rejected on {} to {}", nic.name, endpoint),
                );
                return;
            }
            _ => {
                if self.active_subflow_ids.lock().unwrap().is_empty() {
                    *self.status.lock().unwrap() = TunnelStatus::ServerUnreachable;
                    *self.status_message.lock().unwrap() =
                        format!("Handshake failed with {}", endpoint);
                }
                return;
            }
        };

        let (mut reader, mut writer) = tcp.into_split();
        let (tx, mut rx) = mpsc::channel::<BondingFrame>(256);
        let if_index = nic.if_index;
        let nic_name = nic.name.clone();

        let client_for_task = self.clone();
        let task = tokio::spawn(async move {
            let client_writer = client_for_task.clone();
            let writer_job = tokio::spawn(async move {
                while let Some(frame) = rx.recv().await {
                    match write_encrypted_frame(&mut writer, &mut sealer, &frame).await {
                        Ok(written) => {
                            client_writer
                                .total_tx_bytes
                                .fetch_add(written as u64, Ordering::Relaxed);
                        }
                        Err(_) => {
                            client_writer.mark_subflow_down(if_index).await;
                            // Failover: re-dispatch dropped frame onto surviving subflows!
                            let _ = client_writer.send_multipath_frame(frame).await;
                            break;
                        }
                    }
                }
            });

            while let Ok(frame) = read_encrypted_frame(&mut reader, &mut opener).await {
                client_for_task.handle_server_frame(frame).await;
            }

            writer_job.abort();
            client_for_task.mark_subflow_down(if_index).await;
        });

        {
            let mut map = self.subflows.lock().await;
            map.insert(
                if_index,
                ClientSubflow {
                    if_index,
                    interface_name: nic_name.clone(),
                    descriptor: SubflowDescriptor {
                        subflow_id: if_index,
                        throughput_bps: nic.download_bps.max(10_000_000),
                        link_capacity_bps: nic.link_speed_bps,
                        rtt_ms: nic.latency_ms.unwrap_or(25.0),
                        loss_pct: nic.packet_loss_pct,
                        is_primary: nic.is_primary_failover,
                        custom_weight: nic.custom_weight,
                        alive: true,
                        inflight_bytes: 0,
                    },
                    tx,
                    abort_handle: task.abort_handle(),
                },
            );
            *self.active_subflow_ids.lock().unwrap() = map.keys().copied().collect();
        }

        let count = self.active_subflow_ids.lock().unwrap().len();
        *self.status.lock().unwrap() = TunnelStatus::Connected;
        *self.status_message.lock().unwrap() =
            format!("Encrypted Multipath Tunnel Connected ({count} Subflows Active)");
        self.logger.success(
            "BONDING",
            format!(
                "Attached encrypted subflow on {} ({}) -> {}",
                nic_name, local_ipv4, endpoint
            ),
        );
    }

    async fn mark_subflow_down(&self, if_index: u32) {
        let removed = self.subflows.lock().await.remove(&if_index);
        let remaining = {
            let mut ids = self.active_subflow_ids.lock().unwrap();
            ids.retain(|x| *x != if_index);
            ids.len()
        };

        if let Some(sf) = removed {
            self.logger.warn(
                "BONDING",
                format!(
                    "Subflow lost on {}; failing over traffic across {} remaining subflow(s)",
                    sf.interface_name, remaining
                ),
            );
        }

        if self.enabled.load(Ordering::Relaxed) {
            if remaining == 0 {
                *self.status.lock().unwrap() = TunnelStatus::ServerUnreachable;
                *self.status_message.lock().unwrap() =
                    "All tunnel subflows disconnected — Direct Multi-Path fallback active".into();
            } else {
                *self.status.lock().unwrap() = TunnelStatus::Connected;
                *self.status_message.lock().unwrap() = format!(
                    "Encrypted Multipath Tunnel Connected ({remaining} Subflows Active)"
                );
            }
        }
    }

    pub async fn send_multipath_frame(&self, mut frame: BondingFrame) -> Result<(), String> {
        let chunk_len = match &frame {
            BondingFrame::StreamChunk { data, .. }
            | BondingFrame::SpeedTestUpChunk { data, .. } => data.len(),
            _ => 512,
        };

        for _ in 0..6 {
            let policy = *self.policy.lock().unwrap();
            let chosen = {
                let map = self.subflows.lock().await;
                if map.is_empty() {
                    return Err("No active bonding subflows".into());
                }
                let descriptors: Vec<SubflowDescriptor> =
                    map.values().map(|s| s.descriptor.clone()).collect();
                let mut sched = self.scheduler.lock().await;
                let id = sched
                    .select_subflow(policy, &descriptors, chunk_len)
                    .or_else(|| descriptors.first().map(|d| d.subflow_id));
                id.and_then(|i| map.get(&i).map(|s| (i, s.tx.clone())))
            };

            if let Some((id, tx)) = chosen {
                match tx.send(frame).await {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        frame = e.0;
                        self.mark_subflow_down(id).await;
                    }
                }
            } else {
                return Err("No eligible subflow".into());
            }
        }
        Err("All subflows failed".into())
    }

    async fn handle_server_frame(&self, frame: BondingFrame) {
        match frame {
            BondingFrame::Pong { sent_ms, .. } => {
                let rtt_ms = now_ms().saturating_sub(sent_ms);
                self.last_rtt_us.store(rtt_ms * 1000, Ordering::Relaxed);
            }
            BondingFrame::TcpOpenAck {
                stream_id,
                ok,
                error,
            } => {
                if let Some(waiter) = self.pending_opens.lock().await.remove(&stream_id) {
                    if ok {
                        let _ = waiter.send(Ok(()));
                    } else {
                        let _ = waiter
                            .send(Err(error.unwrap_or_else(|| "Remote open failed".into())));
                    }
                }
            }
            BondingFrame::StreamChunk {
                stream_id,
                offset,
                eof,
                data,
            } => {
                self.total_rx_bytes
                    .fetch_add(data.len() as u64, Ordering::Relaxed);
                let mut deliver = None;
                let mut done = false;
                {
                    let mut streams = self.streams.lock().await;
                    if let Some(st) = streams.get_mut(&stream_id) {
                        if let Ok(ordered) = st.reassembler.push_chunk(offset, data, eof) {
                            self.reassembly_depth
                                .store(st.reassembler.queued_chunks(), Ordering::Relaxed);
                            if !ordered.is_empty() {
                                deliver = Some((st.down_tx.clone(), ordered));
                            }
                            done = st.reassembler.is_complete();
                        }
                    }
                }
                if let Some((tx, ordered)) = deliver {
                    let _ = tx.send(ordered).await;
                }
                if done {
                    self.streams.lock().await.remove(&stream_id);
                }
            }
            BondingFrame::StreamClose { stream_id, .. } => {
                self.streams.lock().await.remove(&stream_id);
            }
            BondingFrame::SpeedTestUpResult {
                stream_id,
                total_bytes,
                elapsed_ms,
            } => {
                if let Some(tx) = self.pending_speedtest_up.lock().await.remove(&stream_id) {
                    let _ = tx.send((total_bytes, elapsed_ms));
                }
            }
            _ => {}
        }
    }

    async fn handle_local_socks5_stream(
        self: Arc<Self>,
        client: &mut TcpStream,
    ) -> std::io::Result<()> {
        let mut first = [0u8; 1];
        if client.peek(&mut first).await? == 0 {
            return Ok(());
        }

        // Dual-Protocol Unified Virtual Gateway:
        // - 0x05 -> SOCKS5 protocol
        // - ASCII ('C', 'G', 'P', etc.) -> HTTP/HTTPS CONNECT or standard HTTP Proxy request
        if first[0] != 0x05 {
            return self.handle_local_http_proxy_stream(client).await;
        }

        let mut hdr = [0u8; 2];
        client.read_exact(&mut hdr).await?;
        if hdr[0] != 0x05 {
            return Ok(());
        }
        let mut methods = vec![0u8; hdr[1] as usize];
        client.read_exact(&mut methods).await?;
        client.write_all(&[0x05, 0x00]).await?;

        let mut req = [0u8; 4];
        client.read_exact(&mut req).await?;
        if req[1] != 0x01 {
            client
                .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
            return Ok(());
        }

        let host = match req[3] {
            0x01 => {
                let mut ip = [0u8; 4];
                client.read_exact(&mut ip).await?;
                Ipv4Addr::from(ip).to_string()
            }
            0x03 => {
                let mut len = [0u8; 1];
                client.read_exact(&mut len).await?;
                let mut dom = vec![0u8; len[0] as usize];
                client.read_exact(&mut dom).await?;
                String::from_utf8_lossy(&dom).to_string()
            }
            _ => return Ok(()),
        };
        let mut port_buf = [0u8; 2];
        client.read_exact(&mut port_buf).await?;
        let port = u16::from_be_bytes(port_buf);

        // If the remote Encrypted Multipath Bonding Server is not connected, route directly via
        // weighted IP_UNICAST_IF interface selection across all enabled Windows NICs!
        if !self.is_tunnel_connected() {
            match self.connect_direct_multipath(&host, port).await {
                Ok(mut upstream) => {
                    client
                        .write_all(&[0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0, 0])
                        .await?;
                    if let Ok((up_b, down_b)) =
                        tokio::io::copy_bidirectional(client, &mut upstream).await
                    {
                        self.total_tx_bytes.fetch_add(up_b, Ordering::Relaxed);
                        self.total_rx_bytes.fetch_add(down_b, Ordering::Relaxed);
                    }
                    return Ok(());
                }
                Err(_) => {
                    let _ = client
                        .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                        .await;
                    return Ok(());
                }
            }
        }

        let stream_id = self.next_stream_id.fetch_add(1, Ordering::Relaxed);
        let (ack_tx, ack_rx) = oneshot::channel();
        let (down_tx, mut down_rx) = mpsc::channel::<Vec<u8>>(256);

        self.pending_opens.lock().await.insert(stream_id, ack_tx);
        self.streams.lock().await.insert(
            stream_id,
            RelayedStream {
                reassembler: StreamReassembler::new(32 * 1024 * 1024),
                down_tx,
            },
        );

        if self
            .send_multipath_frame(BondingFrame::TcpOpen {
                stream_id,
                target_host: host,
                target_port: port,
            })
            .await
            .is_err()
        {
            self.pending_opens.lock().await.remove(&stream_id);
            self.streams.lock().await.remove(&stream_id);
            let _ = client
                .write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await;
            return Ok(());
        }

        match tokio::time::timeout(Duration::from_secs(12), ack_rx).await {
            Ok(Ok(Ok(()))) => {
                client
                    .write_all(&[0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0, 0])
                    .await?;
            }
            _ => {
                self.streams.lock().await.remove(&stream_id);
                let _ = client
                    .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await;
                return Ok(());
            }
        }

        let (mut c_read, mut c_write) = client.split();
        let client_up = self.clone();

        let up_fut = async move {
            let mut buf = vec![0u8; DEFAULT_CHUNK_SIZE];
            let mut offset = 0u64;
            loop {
                match c_read.read(&mut buf).await {
                    Ok(0) => {
                        let _ = client_up
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
                        if client_up.send_multipath_frame(frame).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        };

        let down_fut = async move {
            while let Some(chunk) = down_rx.recv().await {
                if c_write.write_all(&chunk).await.is_err() {
                    break;
                }
            }
            let _ = c_write.shutdown().await;
        };

        tokio::join!(up_fut, down_fut);
        self.streams.lock().await.remove(&stream_id);
        Ok(())
    }

    async fn handle_local_http_proxy_stream(
        self: Arc<Self>,
        client: &mut TcpStream,
    ) -> std::io::Result<()> {
        let mut req_buf = vec![0u8; 8192];
        let n = client.read(&mut req_buf).await?;
        if n == 0 {
            return Ok(());
        }
        let req_str = String::from_utf8_lossy(&req_buf[..n]);
        let first_line = req_str.lines().next().unwrap_or("");
        let parts: Vec<&str> = first_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Ok(());
        }

        let method = parts[0];
        let target = parts[1];

        let (host, port, is_connect) = if method.eq_ignore_ascii_case("CONNECT") {
            let (h, p_str) = target.rsplit_once(':').unwrap_or((target, "443"));
            let p = p_str.parse::<u16>().unwrap_or(443);
            (h.to_string(), p, true)
        } else if let Some(rest) = target.strip_prefix("http://") {
            let authority = rest.split('/').next().unwrap_or(rest);
            if let Some((h, p_str)) = authority.rsplit_once(':') {
                (h.to_string(), p_str.parse::<u16>().unwrap_or(80), false)
            } else {
                (authority.to_string(), 80, false)
            }
        } else {
            return Ok(());
        };

        // If encrypted multipath VPS tunnel is connected, relay through bonded tunnel!
        if self.is_tunnel_connected() {
            let stream_id = self.next_stream_id.fetch_add(1, Ordering::Relaxed);
            let (ack_tx, ack_rx) = oneshot::channel();
            let (down_tx, mut down_rx) = mpsc::channel::<Vec<u8>>(256);

            self.pending_opens.lock().await.insert(stream_id, ack_tx);
            self.streams.lock().await.insert(
                stream_id,
                RelayedStream {
                    reassembler: StreamReassembler::new(32 * 1024 * 1024),
                    down_tx,
                },
            );

            if self
                .send_multipath_frame(BondingFrame::TcpOpen {
                    stream_id,
                    target_host: host.clone(),
                    target_port: port,
                })
                .await
                .is_ok()
            {
                if let Ok(Ok(Ok(()))) =
                    tokio::time::timeout(Duration::from_secs(10), ack_rx).await
                {
                    let mut initial_offset = 0u64;
                    if is_connect {
                        client
                            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                            .await?;
                    } else {
                        let _ = self
                            .send_multipath_frame(BondingFrame::StreamChunk {
                                stream_id,
                                offset: 0,
                                eof: false,
                                data: req_buf[..n].to_vec(),
                            })
                            .await;
                        initial_offset = n as u64;
                    }

                    let (mut c_read, mut c_write) = client.split();
                    let client_up = self.clone();
                    let up_fut = async move {
                        let mut buf = vec![0u8; DEFAULT_CHUNK_SIZE];
                        let mut offset = initial_offset;
                        loop {
                            match c_read.read(&mut buf).await {
                                Ok(0) => {
                                    let _ = client_up
                                        .send_multipath_frame(BondingFrame::StreamChunk {
                                            stream_id,
                                            offset,
                                            eof: true,
                                            data: Vec::new(),
                                        })
                                        .await;
                                    break;
                                }
                                Ok(read_n) => {
                                    let frame = BondingFrame::StreamChunk {
                                        stream_id,
                                        offset,
                                        eof: false,
                                        data: buf[..read_n].to_vec(),
                                    };
                                    offset += read_n as u64;
                                    if client_up.send_multipath_frame(frame).await.is_err() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    };
                    let down_fut = async move {
                        while let Some(chunk) = down_rx.recv().await {
                            if c_write.write_all(&chunk).await.is_err() {
                                break;
                            }
                        }
                        let _ = c_write.shutdown().await;
                    };
                    tokio::join!(up_fut, down_fut);
                    self.streams.lock().await.remove(&stream_id);
                    return Ok(());
                }
            }
            self.pending_opens.lock().await.remove(&stream_id);
            self.streams.lock().await.remove(&stream_id);
        }

        // Direct Multi-Path IP_UNICAST_IF mode across enabled Windows adapters
        match self.connect_direct_multipath(&host, port).await {
            Ok(mut upstream) => {
                if is_connect {
                    client
                        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                        .await?;
                } else {
                    upstream.write_all(&req_buf[..n]).await?;
                }
                if let Ok((up_b, down_b)) =
                    tokio::io::copy_bidirectional(client, &mut upstream).await
                {
                    self.total_tx_bytes.fetch_add(up_b, Ordering::Relaxed);
                    self.total_rx_bytes.fetch_add(down_b, Ordering::Relaxed);
                }
            }
            Err(_) => {
                let _ = client
                    .write_all(b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n")
                    .await;
            }
        }
        Ok(())
    }

    async fn connect_direct_multipath(
        &self,
        host: &str,
        port: u16,
    ) -> Result<TcpStream, String> {
        let candidates = self.direct_interfaces.lock().unwrap().clone();
        if !candidates.is_empty() && self.bonding_active.load(Ordering::Relaxed) {
            let start = self.direct_rr_cursor.fetch_add(1, Ordering::Relaxed);
            for offset in 0..candidates.len() {
                let (if_index, ip4, _, _) = &candidates[(start + offset) % candidates.len()];
                if let Ok(stream) = resolve_and_connect_on_interface(
                    host,
                    port,
                    *ip4,
                    *if_index,
                    Duration::from_secs(6),
                )
                .await
                {
                    let _ = stream.set_nodelay(true);
                    return Ok(stream);
                }
            }
        }
        tokio::time::timeout(Duration::from_secs(6), TcpStream::connect((host, port)))
            .await
            .map_err(|_| "Direct connection timeout".to_string())?
            .map_err(|e| e.to_string())
    }

    /// Runs a direct multipath tunnel download & upload benchmark against the connected Bonding Server!
    pub async fn benchmark_tunnel_throughput(
        &self,
        down_bytes: u64,
        up_bytes: u64,
    ) -> Result<(u64, u64), String> {
        if !self.is_tunnel_connected() {
            return Err("Bonding tunnel is not connected".into());
        }

        // 1. Download burst across all active subflows
        let down_stream_id = self.next_stream_id.fetch_add(1, Ordering::Relaxed);
        let (down_tx, mut down_rx) = mpsc::channel::<Vec<u8>>(512);
        self.streams.lock().await.insert(
            down_stream_id,
            RelayedStream {
                reassembler: StreamReassembler::new(64 * 1024 * 1024),
                down_tx,
            },
        );

        let dl_start = std::time::Instant::now();
        self.send_multipath_frame(BondingFrame::SpeedTestDownRequest {
            stream_id: down_stream_id,
            total_bytes: down_bytes,
        })
        .await?;

        let mut received = 0u64;
        while let Ok(Some(chunk)) =
            tokio::time::timeout(Duration::from_secs(15), down_rx.recv()).await
        {
            received += chunk.len() as u64;
            if received >= down_bytes {
                break;
            }
        }
        let dl_elapsed = dl_start.elapsed().as_secs_f64().max(0.001);
        let dl_bps = ((received as f64 * 8.0) / dl_elapsed) as u64;
        self.streams.lock().await.remove(&down_stream_id);

        // 2. Upload burst across all active subflows
        let up_stream_id = self.next_stream_id.fetch_add(1, Ordering::Relaxed);
        let (res_tx, res_rx) = oneshot::channel();
        self.pending_speedtest_up
            .lock()
            .await
            .insert(up_stream_id, res_tx);

        let up_start = std::time::Instant::now();
        let payload = vec![0x5Au8; DEFAULT_CHUNK_SIZE];
        let mut sent = 0u64;
        while sent < up_bytes {
            let take = ((up_bytes - sent) as usize).min(DEFAULT_CHUNK_SIZE);
            let eof = sent + (take as u64) >= up_bytes;
            self.send_multipath_frame(BondingFrame::SpeedTestUpChunk {
                stream_id: up_stream_id,
                offset: sent,
                eof,
                data: payload[..take].to_vec(),
            })
            .await?;
            sent += take as u64;
        }

        let ul_bps = match tokio::time::timeout(Duration::from_secs(15), res_rx).await {
            Ok(Ok((srv_bytes, srv_ms))) => {
                ((srv_bytes as f64 * 8.0) / ((srv_ms as f64 / 1000.0).max(0.001))) as u64
            }
            _ => {
                let ul_elapsed = up_start.elapsed().as_secs_f64().max(0.001);
                ((sent as f64 * 8.0) / ul_elapsed) as u64
            }
        };

        Ok((dl_bps, ul_bps))
    }
}

