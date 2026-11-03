use crate::bonding::client::BondingTunnelClient;
use crate::db::Database;
use crate::downloader::validator::{
    extract_filename, sanitize_windows_filename, unique_destination_path, validate_download_url,
};
use crate::logger::AppLogger;
use crate::models::{
    now_ms, ActivePathContribution, AddDownloadRequest, DownloadItem, DownloadPriority,
    DownloadSegmentInfo, DownloadStatus, InterfaceKind, UrlProbeResult,
};
use crate::network::monitor::NetworkMonitor;
use futures_util::StreamExt;
use md5::Md5;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinSet;

const MIN_SPLITTABLE_REMAINING_BYTES: u64 = 2 * 1024 * 1024; // 2 MiB work-stealing threshold

struct SharedSegmentState {
    start_byte: u64,
    end_byte: AtomicU64, // Dynamic so work-stealing can shrink a slow segment's end_byte safely!
    downloaded_bytes: AtomicU64,
    status: StdMutex<String>,
    assigned_if_id: StdMutex<Option<String>>,
    assigned_if_name: StdMutex<Option<String>>,
    speed_bps: AtomicU64,
    retries: AtomicU64,
}

struct ActiveTaskControl {
    cancel_flag: AtomicBool,
    pause_flag: AtomicBool,
    speed_limit_bps: AtomicU64, // 0 = unlimited
    stop_notify: Notify,
}

#[derive(Clone)]
struct PathRoute {
    interface_id: String,
    interface_name: String,
    kind: InterfaceKind,
    weight_pct: f64,
    #[allow(dead_code)]
    proxy_port: Option<u16>,
    via_tunnel: bool,
    client: reqwest::Client,
}

pub struct DownloadEngine {
    db: Arc<Database>,
    logger: AppLogger,
    monitor: Arc<NetworkMonitor>,
    bonding_client: Arc<BondingTunnelClient>,
    items: Mutex<Vec<DownloadItem>>,
    controls: Mutex<HashMap<String, Arc<ActiveTaskControl>>>,
    app_handle: StdMutex<Option<AppHandle>>,
}

impl DownloadEngine {
    pub fn new(
        db: Arc<Database>,
        logger: AppLogger,
        monitor: Arc<NetworkMonitor>,
        bonding_client: Arc<BondingTunnelClient>,
    ) -> Arc<Self> {
        let initial_items = db.load_all_downloads();
        Arc::new(Self {
            db,
            logger,
            monitor,
            bonding_client,
            items: Mutex::new(initial_items),
            controls: Mutex::new(HashMap::new()),
            app_handle: StdMutex::new(None),
        })
    }

    pub fn attach_app_handle(&self, handle: AppHandle) {
        if let Ok(mut g) = self.app_handle.lock() {
            *g = Some(handle);
        }
    }

    pub fn start_scheduler_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(600));
            loop {
                interval.tick().await;
                self.clone().tick_queue().await;
            }
        });
    }

    pub async fn list_downloads(&self) -> Vec<DownloadItem> {
        self.items.lock().await.clone()
    }

    async fn emit_downloads_updated(&self) {
        let snapshot = self.items.lock().await.clone();
        if let Ok(g) = self.app_handle.lock() {
            if let Some(ref app) = *g {
                let _ = app.emit("downloads-updated", &snapshot);
            }
        }
    }

    fn send_completion_notification(&self, title: &str, body: &str) {
        if !self.db.get_settings().notify_on_complete {
            return;
        }
        if let Ok(g) = self.app_handle.lock() {
            if let Some(ref app) = *g {
                let _ = app.notification().builder().title(title).body(body).show();
            }
        }
    }

    pub async fn probe_url(&self, raw_url: &str) -> Result<UrlProbeResult, String> {
        let parsed = validate_download_url(raw_url)?;
        let (_, routes) = self.resolve_multipath_routes().await;
        let client = routes
            .first()
            .map(|r| r.client.clone())
            .or_else(|| build_http_client(None).ok())
            .ok_or_else(|| "Failed to initialize HTTP probe client".to_string())?;

        // First try HEAD, then fallback to GET Range: bytes=0-0
        let mut total_bytes: Option<u64> = None;
        let mut supports_range = false;
        let mut content_type: Option<String> = None;
        let mut content_disp: Option<String> = None;

        if let Ok(resp) = client
            .head(parsed.as_str())
            .timeout(Duration::from_secs(8))
            .send()
            .await
        {
            if resp.status().is_success() {
                total_bytes = resp
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());
                supports_range = resp
                    .headers()
                    .get(reqwest::header::ACCEPT_RANGES)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.eq_ignore_ascii_case("bytes"))
                    .unwrap_or(false);
                content_type = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                content_disp = resp
                    .headers()
                    .get(reqwest::header::CONTENT_DISPOSITION)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
            }
        }

        // Verify true byte-range capability via Range: bytes=0-0 if still uncertain
        if !supports_range || total_bytes.is_none() {
            if let Ok(resp) = client
                .get(parsed.as_str())
                .header(reqwest::header::RANGE, "bytes=0-0")
                .timeout(Duration::from_secs(8))
                .send()
                .await
            {
                if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                    supports_range = true;
                    if let Some(cr) = resp
                        .headers()
                        .get(reqwest::header::CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok())
                    {
                        if let Some((_, tot_str)) = cr.rsplit_once('/') {
                            if let Ok(tot) = tot_str.trim().parse::<u64>() {
                                total_bytes = Some(tot);
                            }
                        }
                    }
                } else if resp.status().is_success() && total_bytes.is_none() {
                    total_bytes = resp
                        .headers()
                        .get(reqwest::header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok());
                }
                if content_type.is_none() {
                    content_type = resp
                        .headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                }
                if content_disp.is_none() {
                    content_disp = resp
                        .headers()
                        .get(reqwest::header::CONTENT_DISPOSITION)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                }
            }
        }

        let suggested_file_name = extract_filename(&parsed, content_disp.as_deref());

        Ok(UrlProbeResult {
            valid: true,
            normalized_url: parsed.to_string(),
            suggested_file_name,
            total_bytes,
            supports_range,
            content_type,
            warning: None,
        })
    }

    pub async fn add_download(
        self: &Arc<Self>,
        req: AddDownloadRequest,
    ) -> Result<DownloadItem, String> {
        let parsed = validate_download_url(&req.url)?;
        let settings = self.db.get_settings();

        let save_dir_str = req
            .save_dir
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| settings.default_download_dir.clone());
        let save_dir_path = PathBuf::from(&save_dir_str);
        std::fs::create_dir_all(&save_dir_path)
            .map_err(|e| format!("Failed to create destination folder: {e}"))?;

        let raw_name = req
            .file_name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .map(|n| sanitize_windows_filename(&n))
            .unwrap_or_else(|| extract_filename(&parsed, None));

        let (final_name, target_path) = unique_destination_path(&save_dir_path, &raw_name);
        let now = now_ms();

        let initial_status = if let Some(sched) = req.scheduled_at_ms {
            if sched > now {
                DownloadStatus::Scheduled
            } else {
                DownloadStatus::Queued
            }
        } else if req.start_immediately {
            DownloadStatus::Queued
        } else {
            DownloadStatus::Paused
        };

        let seg_count = req
            .max_segments
            .unwrap_or(settings.default_segments_per_download)
            .clamp(1, 32);

        // Store placeholder segment count hint via empty segments until probed
        let item = DownloadItem {
            id: uuid::Uuid::new_v4().to_string(),
            url: parsed.to_string(),
            file_name: final_name.clone(),
            save_dir: save_dir_str,
            target_path: target_path.to_string_lossy().to_string(),
            total_bytes: None,
            downloaded_bytes: 0,
            status: initial_status,
            priority: req.priority,
            supports_range: false,
            speed_bps: 0,
            avg_speed_bps: 0,
            eta_seconds: None,
            speed_limit_bps: req.speed_limit_bps,
            scheduled_at_ms: req.scheduled_at_ms,
            expected_checksum: req
                .expected_checksum
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_ascii_lowercase()),
            checksum_algorithm: req.checksum_algorithm,
            computed_checksum: None,
            checksum_verified: None,
            error_message: None,
            retry_count: 0,
            max_retries: settings.max_retries,
            created_at_ms: now,
            completed_at_ms: None,
            used_bonding_tunnel: false,
            // Temporarily stash desired seg_count as a single marker if >0? Even simpler: probe uses settings or default_segments, or let's store desired segments cleanly
            segments: Vec::with_capacity(seg_count),
            active_paths: Vec::new(),
        };

        self.db.save_download(&item);
        self.items.lock().await.insert(0, item.clone());
        self.logger.info(
            "DOWNLOAD",
            format!("Queued download '{}' ({})", final_name, parsed),
        );
        self.emit_downloads_updated().await;

        let engine_clone = self.clone();
        tokio::spawn(async move {
            engine_clone.tick_queue().await;
        });

        Ok(item)
    }

    pub async fn pause_download(&self, download_id: &str) -> Result<(), String> {
        let had_active_ctrl = {
            let controls = self.controls.lock().await;
            if let Some(ctrl) = controls.get(download_id) {
                ctrl.pause_flag.store(true, Ordering::SeqCst);
                ctrl.stop_notify.notify_waiters();
                true
            } else {
                false
            }
        };

        // Wait briefly for active workers to flush their .nfdl file bytes and checkpoint exact offsets to SQLite
        if had_active_ctrl {
            for _ in 0..30 {
                if !self.controls.lock().await.contains_key(download_id) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }

        let mut paused_info: Option<(String, u64)> = None;
        let mut items = self.items.lock().await;
        if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
            if !matches!(
                item.status,
                DownloadStatus::Completed | DownloadStatus::Cancelled
            ) {
                item.status = DownloadStatus::Paused;
                item.speed_bps = 0;
                item.eta_seconds = None;
                item.active_paths.clear();
                for seg in &mut item.segments {
                    if seg.status == "Active" {
                        seg.status = "Pending".to_string();
                    }
                    seg.speed_bps = 0;
                }
                self.db.save_download(item);
                paused_info = Some((item.file_name.clone(), item.downloaded_bytes));
            }
        }
        drop(items);

        if let Some((name, bytes)) = paused_info {
            self.logger.info(
                "DOWNLOAD",
                format!(
                    "Paused '{}' at {} bytes (checkpoint saved for exact resume)",
                    name, bytes
                ),
            );
        }
        self.emit_downloads_updated().await;
        Ok(())
    }

    pub async fn resume_download(self: &Arc<Self>, download_id: &str) -> Result<(), String> {
        // Ensure any winding-down pause task has completely exited before re-queueing
        for _ in 0..25 {
            if !self.controls.lock().await.contains_key(download_id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let mut resumed_info: Option<(String, u64)> = None;
        {
            let mut items = self.items.lock().await;
            if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
                if matches!(
                    item.status,
                    DownloadStatus::Paused | DownloadStatus::Scheduled | DownloadStatus::Failed
                ) {
                    item.status = DownloadStatus::Queued;
                    item.scheduled_at_ms = None;
                    item.error_message = None;
                    for seg in &mut item.segments {
                        if seg.status != "Completed" {
                            seg.status = "Pending".to_string();
                        }
                    }
                    self.db.save_download(item);
                    resumed_info = Some((item.file_name.clone(), item.downloaded_bytes));
                }
            }
        }
        if let Some((name, bytes)) = resumed_info {
            self.logger.info(
                "DOWNLOAD",
                format!("Resuming '{}' from byte offset {}", name, bytes),
            );
        }
        self.emit_downloads_updated().await;
        self.clone().tick_queue().await;
        Ok(())
    }

    pub async fn retry_download(self: &Arc<Self>, download_id: &str) -> Result<(), String> {
        {
            let mut items = self.items.lock().await;
            if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
                item.status = DownloadStatus::Queued;
                item.error_message = None;
                item.retry_count = 0;
                for seg in &mut item.segments {
                    if seg.status != "Completed" {
                        seg.status = "Pending".to_string();
                    }
                }
                self.db.save_download(item);
            }
        }
        self.emit_downloads_updated().await;
        self.clone().tick_queue().await;
        Ok(())
    }

    pub async fn cancel_download(&self, download_id: &str, delete_files: bool) -> Result<(), String> {
        if let Some(ctrl) = self.controls.lock().await.remove(download_id) {
            ctrl.cancel_flag.store(true, Ordering::SeqCst);
            ctrl.stop_notify.notify_waiters();
        }

        let removed = {
            let mut items = self.items.lock().await;
            if let Some(pos) = items.iter().position(|d| d.id == download_id) {
                Some(items.remove(pos))
            } else {
                None
            }
        };

        if let Some(item) = removed {
            self.db.delete_download(&item.id);
            if delete_files {
                let part_path = format!("{}.nfdl", item.target_path);
                let _ = tokio::fs::remove_file(&part_path).await;
                if item.status != DownloadStatus::Completed {
                    let _ = tokio::fs::remove_file(&item.target_path).await;
                }
            }
            self.logger
                .info("DOWNLOAD", format!("Removed download '{}'", item.file_name));
        }

        self.emit_downloads_updated().await;
        Ok(())
    }

    pub async fn update_priority(&self, download_id: &str, priority: DownloadPriority) {
        let mut items = self.items.lock().await;
        if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
            item.priority = priority;
            self.db.save_download(item);
        }
        drop(items);
        self.emit_downloads_updated().await;
    }

    pub async fn update_speed_limit(&self, download_id: &str, limit_bps: Option<u64>) {
        if let Some(ctrl) = self.controls.lock().await.get(download_id) {
            ctrl.speed_limit_bps
                .store(limit_bps.unwrap_or(0), Ordering::Relaxed);
        }
        let mut items = self.items.lock().await;
        if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
            item.speed_limit_bps = limit_bps;
            self.db.save_download(item);
        }
        drop(items);
        self.emit_downloads_updated().await;
    }

    pub async fn pause_all(&self) {
        let ids: Vec<String> = self
            .items
            .lock()
            .await
            .iter()
            .filter(|d| {
                matches!(
                    d.status,
                    DownloadStatus::Downloading | DownloadStatus::Queued | DownloadStatus::Probing
                )
            })
            .map(|d| d.id.clone())
            .collect();
        for id in ids {
            let _ = self.pause_download(&id).await;
        }
    }

    pub async fn resume_all(self: &Arc<Self>) {
        let ids: Vec<String> = self
            .items
            .lock()
            .await
            .iter()
            .filter(|d| matches!(d.status, DownloadStatus::Paused | DownloadStatus::Failed))
            .map(|d| d.id.clone())
            .collect();
        for id in ids {
            let _ = self.resume_download(&id).await;
        }
    }

    pub async fn clear_completed(&self) {
        let completed_ids: Vec<String> = self
            .items
            .lock()
            .await
            .iter()
            .filter(|d| d.status == DownloadStatus::Completed)
            .map(|d| d.id.clone())
            .collect();
        {
            let mut items = self.items.lock().await;
            items.retain(|d| d.status != DownloadStatus::Completed);
        }
        for id in completed_ids {
            self.db.delete_download(&id);
        }
        self.emit_downloads_updated().await;
    }

    async fn tick_queue(self: Arc<Self>) {
        let settings = self.db.get_settings();
        let now = now_ms();
        let mut to_launch: Vec<String> = Vec::new();

        {
            let mut items = self.items.lock().await;
            let active_controls = self.controls.lock().await;

            // Promote due Scheduled items to Queued
            for item in items.iter_mut() {
                if item.status == DownloadStatus::Scheduled {
                    if let Some(sched) = item.scheduled_at_ms {
                        if sched <= now {
                            item.status = DownloadStatus::Queued;
                            self.db.save_download(item);
                        }
                    }
                }
            }

            let running_count = active_controls.len();
            if running_count < settings.max_concurrent_downloads {
                let slots = settings.max_concurrent_downloads - running_count;
                let mut candidates: Vec<&mut DownloadItem> = items
                    .iter_mut()
                    .filter(|d| {
                        d.status == DownloadStatus::Queued && !active_controls.contains_key(&d.id)
                    })
                    .collect();

                // Highest priority first, then oldest created_at_ms
                candidates.sort_by(|a, b| {
                    b.priority
                        .rank()
                        .cmp(&a.priority.rank())
                        .then_with(|| a.created_at_ms.cmp(&b.created_at_ms))
                });

                for cand in candidates.into_iter().take(slots) {
                    cand.status = DownloadStatus::Probing;
                    to_launch.push(cand.id.clone());
                }
            }
        }

        for id in to_launch {
            let engine = self.clone();
            tokio::spawn(async move {
                engine.execute_download_task(id).await;
            });
        }
    }

    /// Builds available multipath HTTP routes:
    /// - If Encrypted Bonding Tunnel is connected: exposes tunnel SOCKS5 path (which stripes 32 KiB chunks across ALL subflows!)
    /// - Plus / Fallback: exposes one `IP_UNICAST_IF`-pinned SOCKS5 HTTP client per enabled Windows network interface!
    async fn resolve_multipath_routes(&self) -> (bool, Vec<PathRoute>) {
        let active_nics = self.monitor.active_bonding_interfaces().await;

        if self.bonding_client.is_tunnel_connected() {
            if let Some(tunnel_port) = self.bonding_client.local_socks_port() {
                if let Ok(tunnel_http) = build_http_client(Some(tunnel_port)) {
                    let mut routes = Vec::new();
                    if active_nics.is_empty() {
                        routes.push(PathRoute {
                            interface_id: "bonded-tunnel".into(),
                            interface_name: "Bonded Multipath Tunnel".into(),
                            kind: InterfaceKind::Other,
                            weight_pct: 100.0,
                            proxy_port: Some(tunnel_port),
                            via_tunnel: true,
                            client: tunnel_http,
                        });
                    } else {
                        for nic in &active_nics {
                            routes.push(PathRoute {
                                interface_id: nic.id.clone(),
                                interface_name: nic.name.clone(),
                                kind: nic.kind,
                                weight_pct: nic.allocated_weight_pct.max(5.0),
                                proxy_port: Some(tunnel_port),
                                via_tunnel: true,
                                client: tunnel_http.clone(),
                            });
                        }
                    }
                    return (true, routes);
                }
            }
        }

        // Direct Multi-Interface IP_UNICAST_IF Mode
        let mut routes = Vec::new();
        for nic in &active_nics {
            if nic.allocated_weight_pct <= 0.1 {
                continue;
            }
            if let Ok(client) = build_http_client(nic.local_proxy_port) {
                routes.push(PathRoute {
                    interface_id: nic.id.clone(),
                    interface_name: nic.name.clone(),
                    kind: nic.kind,
                    weight_pct: nic.allocated_weight_pct,
                    proxy_port: nic.local_proxy_port,
                    via_tunnel: false,
                    client,
                });
            }
        }

        if routes.is_empty() {
            if let Ok(fallback) = build_http_client(None) {
                routes.push(PathRoute {
                    interface_id: "default-os".into(),
                    interface_name: "Default Windows Route".into(),
                    kind: InterfaceKind::Other,
                    weight_pct: 100.0,
                    proxy_port: None,
                    via_tunnel: false,
                    client: fallback,
                });
            }
        }

        (false, routes)
    }

    async fn execute_download_task(self: Arc<Self>, download_id: String) {
        let (url, target_path, initial_limit) = {
            let items = self.items.lock().await;
            let Some(item) = items.iter().find(|d| d.id == download_id) else {
                return;
            };
            (
                item.url.clone(),
                item.target_path.clone(),
                item.speed_limit_bps.unwrap_or(0),
            )
        };

        let ctrl = Arc::new(ActiveTaskControl {
            cancel_flag: AtomicBool::new(false),
            pause_flag: AtomicBool::new(false),
            speed_limit_bps: AtomicU64::new(initial_limit),
            stop_notify: Notify::new(),
        });
        self.controls
            .lock()
            .await
            .insert(download_id.clone(), ctrl.clone());
        self.emit_downloads_updated().await;

        let result = self
            .run_segmented_transfer(&download_id, &url, &target_path, ctrl.clone())
            .await;

        self.controls.lock().await.remove(&download_id);

        match result {
            Ok(()) => {
                let file_name = {
                    let mut items = self.items.lock().await;
                    if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
                        item.status = DownloadStatus::Completed;
                        item.speed_bps = 0;
                        item.eta_seconds = Some(0);
                        item.completed_at_ms = Some(now_ms());
                        item.active_paths.clear();
                        self.db.save_download(item);
                        Some(item.file_name.clone())
                    } else {
                        None
                    }
                };
                if let Some(name) = file_name {
                    self.logger
                        .success("DOWNLOAD", format!("Completed download '{name}'"));
                    self.send_completion_notification(
                        "NetFusion — Download Complete",
                        &format!("{name} finished downloading successfully."),
                    );
                }
            }
            Err(err_msg) => {
                if ctrl.cancel_flag.load(Ordering::Relaxed) {
                    return;
                }
                if ctrl.pause_flag.load(Ordering::Relaxed) {
                    let mut items = self.items.lock().await;
                    if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
                        item.status = DownloadStatus::Paused;
                        item.speed_bps = 0;
                        item.eta_seconds = None;
                        item.active_paths.clear();
                        for seg in &mut item.segments {
                            if seg.status == "Active" {
                                seg.status = "Pending".to_string();
                            }
                            seg.speed_bps = 0;
                        }
                        self.db.save_download(item);
                    }
                } else {
                    let settings = self.db.get_settings();
                    let mut should_requeue = false;
                    let mut failed_name = None;
                    {
                        let mut items = self.items.lock().await;
                        if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
                            item.speed_bps = 0;
                            item.active_paths.clear();
                            if settings.auto_retry_downloads && item.retry_count < item.max_retries
                            {
                                item.retry_count += 1;
                                item.status = DownloadStatus::Queued;
                                item.error_message = Some(format!(
                                    "Auto-retry {}/{}: {}",
                                    item.retry_count, item.max_retries, err_msg
                                ));
                                should_requeue = true;
                            } else {
                                item.status = DownloadStatus::Failed;
                                item.error_message = Some(err_msg.clone());
                                failed_name = Some(item.file_name.clone());
                            }
                            self.db.save_download(item);
                        }
                    }
                    self.logger
                        .error("DOWNLOAD", format!("Download error: {err_msg}"));
                    if let Some(name) = failed_name {
                        self.send_completion_notification(
                            "NetFusion — Download Failed",
                            &format!("{name}: {err_msg}"),
                        );
                    }
                    if should_requeue {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }

        self.emit_downloads_updated().await;
    }

    async fn run_segmented_transfer(
        &self,
        download_id: &str,
        url: &str,
        target_path: &str,
        ctrl: Arc<ActiveTaskControl>,
    ) -> Result<(), String> {
        let (via_tunnel, routes) = self.resolve_multipath_routes().await;
        if routes.is_empty() {
            return Err("No active network connection available".into());
        }

        // Probe metadata if we don't yet have persisted segments
        let (mut total_bytes, mut supports_range, mut existing_segments, existing_downloaded) = {
            let items = self.items.lock().await;
            let item = items
                .iter()
                .find(|d| d.id == download_id)
                .ok_or("Download removed")?;
            (
                item.total_bytes,
                item.supports_range,
                item.segments.clone(),
                item.downloaded_bytes,
            )
        };

        let settings = self.db.get_settings();
        let part_path = PathBuf::from(format!("{target_path}.nfdl"));
        if let Some(parent) = part_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        // Verify partial file on disk so resume only uses bytes genuinely present in .nfdl
        let disk_len = tokio::fs::metadata(&part_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        if disk_len == 0 && !existing_segments.is_empty() {
            for seg in &mut existing_segments {
                seg.downloaded_bytes = 0;
                seg.status = "Pending".to_string();
            }
        }

        let has_saved_progress = existing_segments.iter().any(|s| s.downloaded_bytes > 0)
            || (disk_len > 0 && existing_downloaded > 0);

        if existing_segments.is_empty() || (!has_saved_progress && total_bytes.is_none()) {
            if let Ok(probe) = self.probe_url(url).await {
                if total_bytes.is_none() {
                    total_bytes = probe.total_bytes;
                }
                supports_range = supports_range || probe.supports_range;
            }
        }

        // Adaptive concurrency: scale segment count based on active network interfaces & file size
        let desired_segments = if supports_range && total_bytes.unwrap_or(0) >= 512 * 1024 {
            let base = settings.default_segments_per_download.clamp(1, 32);
            let scaled = if settings.adaptive_concurrency {
                base.max(routes.len() * 3).min(24)
            } else {
                base
            };
            let max_by_size = ((total_bytes.unwrap() / (256 * 1024)) as usize).max(1);
            scaled.min(max_by_size)
        } else {
            1
        };

        // Initialize or restore segments (preserving any existing paused segments!)
        let initial_seg_infos: Vec<DownloadSegmentInfo> = if !existing_segments.is_empty() {
            existing_segments
                .into_iter()
                .map(|mut s| {
                    if s.end_byte > 0 && s.start_byte + s.downloaded_bytes > s.end_byte {
                        s.status = "Completed".into();
                    } else if s.status != "Completed" {
                        s.status = "Pending".into();
                    }
                    s.speed_bps = 0;
                    s
                })
                .collect()
        } else if let Some(total) = total_bytes {
            if supports_range && desired_segments > 1 && total > 0 {
                let chunk = total / desired_segments as u64;
                let mut v = Vec::with_capacity(desired_segments);
                for idx in 0..desired_segments {
                    let start = idx as u64 * chunk;
                    let end = if idx + 1 == desired_segments {
                        total - 1
                    } else {
                        (idx as u64 + 1) * chunk - 1
                    };
                    v.push(DownloadSegmentInfo {
                        index: idx,
                        start_byte: start,
                        end_byte: end,
                        downloaded_bytes: 0,
                        status: "Pending".into(),
                        assigned_interface_id: None,
                        assigned_interface_name: None,
                        speed_bps: 0,
                        retries: 0,
                    });
                }
                v
            } else {
                let resumed_bytes = if disk_len > 0 {
                    existing_downloaded.min(disk_len).min(total)
                } else {
                    0
                };
                vec![DownloadSegmentInfo {
                    index: 0,
                    start_byte: 0,
                    end_byte: total.saturating_sub(1),
                    downloaded_bytes: resumed_bytes,
                    status: "Pending".into(),
                    assigned_interface_id: None,
                    assigned_interface_name: None,
                    speed_bps: 0,
                    retries: 0,
                }]
            }
        } else {
            let resumed_bytes = if disk_len > 0 {
                existing_downloaded.min(disk_len)
            } else {
                0
            };
            vec![DownloadSegmentInfo {
                index: 0,
                start_byte: 0,
                end_byte: 0,
                downloaded_bytes: resumed_bytes,
                status: "Pending".into(),
                assigned_interface_id: None,
                assigned_interface_name: None,
                speed_bps: 0,
                retries: 0,
            }]
        };

        // Ensure .nfdl file exists and pre-allocate length if known without truncating existing bytes
        {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .read(true)
                .open(&part_path)
                .await
                .map_err(|e| format!("Cannot open temporary file: {e}"))?;
            if let Some(tot) = total_bytes {
                let cur_len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
                if cur_len < tot {
                    file.set_len(tot)
                        .await
                        .map_err(|e| format!("Disk pre-allocation failed: {e}"))?;
                }
            }
        }

        // Update item status to Downloading
        {
            let mut items = self.items.lock().await;
            if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
                item.status = DownloadStatus::Downloading;
                item.total_bytes = total_bytes;
                item.supports_range = supports_range;
                item.used_bonding_tunnel = via_tunnel;
                item.segments = initial_seg_infos.clone();
                self.db.save_download(item);
            }
        }
        self.emit_downloads_updated().await;

        // Shared segment table supporting dynamic work-stealing!
        let shared_segments: Arc<Mutex<Vec<Arc<SharedSegmentState>>>> = Arc::new(Mutex::new(
            initial_seg_infos
                .iter()
                .map(|s| {
                    Arc::new(SharedSegmentState {
                        start_byte: s.start_byte,
                        end_byte: AtomicU64::new(s.end_byte),
                        downloaded_bytes: AtomicU64::new(s.downloaded_bytes),
                        status: StdMutex::new(if s.status == "Completed" {
                            "Completed".into()
                        } else {
                            "Pending".into()
                        }),
                        assigned_if_id: StdMutex::new(None),
                        assigned_if_name: StdMutex::new(None),
                        speed_bps: AtomicU64::new(0),
                        retries: AtomicU64::new(s.retries as u64),
                    })
                })
                .collect(),
        ));

        // Per-interface byte counters for live ActivePathContribution telemetry
        let path_bytes: Arc<StdMutex<HashMap<String, (String, InterfaceKind, u64, bool)>>> =
            Arc::new(StdMutex::new(HashMap::new()));

        let started_at = Instant::now();
        let initial_done: u64 = initial_seg_infos.iter().map(|s| s.downloaded_bytes).sum();

        // Spawn live progress & SQLite checkpoint reporter task (every 400ms)
        let stop_reporter = Arc::new(AtomicBool::new(false));
        let stop_rep_clone = stop_reporter.clone();
        let segs_rep = shared_segments.clone();
        let paths_rep = path_bytes.clone();
        let dl_id_rep = download_id.to_string();
        let db_rep = self.db.clone();
        let items_rep = &self.items;

        // Build weighted route ring so workers pick interfaces proportionally to allocated_weight_pct!
        let weighted_routes = build_weighted_route_ring(&routes);
        let worker_count = if supports_range && total_bytes.is_some() {
            desired_segments.min(16).max(1)
        } else {
            1
        };

        let mut workers = JoinSet::new();
        for w_idx in 0..worker_count {
            let route = weighted_routes[w_idx % weighted_routes.len()].clone();
            let all_routes = routes.clone();
            let segs_ref = shared_segments.clone();
            let path_bytes_ref = path_bytes.clone();
            let ctrl_ref = ctrl.clone();
            let url_owned = url.to_string();
            let part_owned = part_path.clone();
            let db_for_limit = self.db.clone();
            let is_range = supports_range && total_bytes.is_some();

            workers.spawn(async move {
                worker_loop(
                    w_idx,
                    route,
                    all_routes,
                    segs_ref,
                    path_bytes_ref,
                    ctrl_ref,
                    url_owned,
                    part_owned,
                    is_range,
                    db_for_limit,
                )
                .await
            });
        }

        // Poll progress while workers run
        let mut prev_bytes = initial_done;
        let mut prev_tick = Instant::now();
        let mut prev_path_totals: HashMap<String, u64> = HashMap::new();
        let mut checkpoint_counter = 0u32;

        loop {
            let sleep_fut = tokio::time::sleep(Duration::from_millis(400));
            tokio::pin!(sleep_fut);

            tokio::select! {
                biased;
                _ = ctrl.stop_notify.notified() => {
                    // Give workers up to 150ms to flush their .nfdl file buffers cleanly, then abort stragglers
                    let drain_deadline = Instant::now() + Duration::from_millis(150);
                    while !workers.is_empty() && Instant::now() < drain_deadline {
                        if tokio::time::timeout(Duration::from_millis(30), workers.join_next()).await.is_err() {
                            break;
                        }
                    }
                    workers.abort_all();
                    break;
                }
                Some(join_res) = workers.join_next() => {
                    if let Ok(Err(worker_err)) = join_res {
                        if !ctrl.pause_flag.load(Ordering::Relaxed) && !ctrl.cancel_flag.load(Ordering::Relaxed) {
                            // Abort remaining workers on unrecoverable error
                            workers.abort_all();
                            stop_rep_clone.store(true, Ordering::Relaxed);
                            self.sync_segments_to_item(&dl_id_rep, &segs_rep, 0, 0, None, Vec::new(), true).await;
                            return Err(worker_err);
                        }
                    }
                    if workers.is_empty() {
                        break;
                    }
                }
                _ = &mut sleep_fut => {
                    let dt = prev_tick.elapsed().as_secs_f64().max(0.1);
                    prev_tick = Instant::now();

                    let current_bytes = sum_downloaded_bytes(&segs_rep).await;
                    let delta = current_bytes.saturating_sub(prev_bytes);
                    prev_bytes = current_bytes;

                    let inst_speed_bps = ((delta as f64 * 8.0) / dt) as u64;
                    let total_elapsed = started_at.elapsed().as_secs_f64().max(0.2);
                    let session_bytes = current_bytes.saturating_sub(initial_done);
                    let avg_speed_bps = ((session_bytes as f64 * 8.0) / total_elapsed) as u64;

                    let eta = total_bytes.and_then(|tot| {
                        if inst_speed_bps > 0 && tot > current_bytes {
                            let rem_bits = (tot - current_bytes) * 8;
                            Some(rem_bits / inst_speed_bps)
                        } else {
                            None
                        }
                    });

                    // Build live ActivePathContribution list
                    let mut active_paths = Vec::new();
                    {
                        let pmap = paths_rep.lock().unwrap();
                        for (if_id, (if_name, kind, contributed, via_tun)) in pmap.iter() {
                            let prev_c = prev_path_totals.insert(if_id.clone(), *contributed).unwrap_or(0);
                            let p_bps = (((contributed.saturating_sub(prev_c)) as f64 * 8.0) / dt) as u64;
                            active_paths.push(ActivePathContribution {
                                interface_id: if_id.clone(),
                                interface_name: if_name.clone(),
                                kind: *kind,
                                speed_bps: p_bps,
                                bytes_contributed: *contributed,
                                via_tunnel: *via_tun,
                            });
                        }
                    }

                    checkpoint_counter += 1;
                    let persist_now = checkpoint_counter % 4 == 0;
                    let _ = (&db_rep, items_rep);
                    self.sync_segments_to_item(
                        &dl_id_rep,
                        &segs_rep,
                        inst_speed_bps,
                        avg_speed_bps,
                        eta,
                        active_paths,
                        persist_now,
                    ).await;
                    self.emit_downloads_updated().await;
                }
            }
        }

        if ctrl.cancel_flag.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        if ctrl.pause_flag.load(Ordering::Relaxed) {
            // Reset any Active segments to Pending while keeping their exact downloaded_bytes
            {
                let segs_guard = segs_rep.lock().await;
                for s in segs_guard.iter() {
                    let mut st = s.status.lock().unwrap();
                    if *st == "Active" {
                        *st = "Pending".into();
                    }
                    s.speed_bps.store(0, Ordering::Relaxed);
                }
            }
            self.sync_segments_to_item(&dl_id_rep, &segs_rep, 0, 0, None, Vec::new(), true)
                .await;
            return Err("Paused".into());
        }

        // Finalize segment state
        let total_elapsed = started_at.elapsed().as_secs_f64().max(0.2);
        let final_bytes = sum_downloaded_bytes(&segs_rep).await;
        let final_avg_bps =
            (((final_bytes.saturating_sub(initial_done)) as f64 * 8.0) / total_elapsed) as u64;
        self.sync_segments_to_item(
            &dl_id_rep,
            &segs_rep,
            0,
            final_avg_bps,
            Some(0),
            Vec::new(),
            true,
        )
        .await;

        // Optional File Integrity Checksum Verification (SHA-256 or MD5)
        let (expected_hash, hash_algo) = {
            let mut items = self.items.lock().await;
            if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
                if item.checksum_algorithm.is_some() {
                    item.status = DownloadStatus::Verifying;
                } else {
                    item.status = DownloadStatus::Merging;
                }
                (
                    item.expected_checksum.clone(),
                    item.checksum_algorithm.clone(),
                )
            } else {
                (None, None)
            }
        };
        self.emit_downloads_updated().await;

        if let Some(algo) = hash_algo {
            let digest = compute_file_checksum(&part_path, &algo).await?;
            let verified = expected_hash
                .as_ref()
                .map(|exp| exp.eq_ignore_ascii_case(&digest));
            {
                let mut items = self.items.lock().await;
                if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
                    item.computed_checksum = Some(digest.clone());
                    item.checksum_verified = verified;
                    self.db.save_download(item);
                }
            }
            if verified == Some(false) {
                return Err(format!(
                    "{algo} integrity checksum mismatch! Expected {}, computed {}",
                    expected_hash.unwrap_or_default(),
                    digest
                ));
            }
        }

        // Atomic rename <target>.nfdl -> <target>
        if Path::new(target_path).exists() {
            let _ = tokio::fs::remove_file(target_path).await;
        }
        tokio::fs::rename(&part_path, target_path)
            .await
            .map_err(|e| format!("Failed to finalize output file: {e}"))?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn sync_segments_to_item(
        &self,
        download_id: &str,
        shared_segments: &Arc<Mutex<Vec<Arc<SharedSegmentState>>>>,
        speed_bps: u64,
        avg_speed_bps: u64,
        eta_seconds: Option<u64>,
        active_paths: Vec<ActivePathContribution>,
        persist_to_sqlite: bool,
    ) {
        let segs_guard = shared_segments.lock().await;
        let mut out_segs = Vec::with_capacity(segs_guard.len());
        let mut total_done = 0u64;
        for (idx, s) in segs_guard.iter().enumerate() {
            let dl = s.downloaded_bytes.load(Ordering::Relaxed);
            total_done += dl;
            out_segs.push(DownloadSegmentInfo {
                index: idx,
                start_byte: s.start_byte,
                end_byte: s.end_byte.load(Ordering::Relaxed),
                downloaded_bytes: dl,
                status: s.status.lock().unwrap().clone(),
                assigned_interface_id: s.assigned_if_id.lock().unwrap().clone(),
                assigned_interface_name: s.assigned_if_name.lock().unwrap().clone(),
                speed_bps: s.speed_bps.load(Ordering::Relaxed),
                retries: s.retries.load(Ordering::Relaxed) as u32,
            });
        }
        drop(segs_guard);

        let mut items = self.items.lock().await;
        if let Some(item) = items.iter_mut().find(|d| d.id == download_id) {
            item.downloaded_bytes = total_done;
            if item.total_bytes.is_none() && !item.supports_range && total_done > 0 {
                // Keep downloaded_bytes updated even for chunked unknown-length streams
            }
            item.speed_bps = speed_bps;
            if avg_speed_bps > 0 {
                item.avg_speed_bps = avg_speed_bps;
            }
            item.eta_seconds = eta_seconds;
            item.segments = out_segs;
            item.active_paths = active_paths;
            if persist_to_sqlite {
                self.db.save_download(item);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn worker_loop(
    worker_idx: usize,
    primary_route: PathRoute,
    fallback_routes: Vec<PathRoute>,
    shared_segments: Arc<Mutex<Vec<Arc<SharedSegmentState>>>>,
    path_bytes: Arc<StdMutex<HashMap<String, (String, InterfaceKind, u64, bool)>>>,
    ctrl: Arc<ActiveTaskControl>,
    url: String,
    part_path: PathBuf,
    is_range_mode: bool,
    db: Arc<Database>,
) -> Result<(), String> {
    let mut current_route = primary_route;

    loop {
        if ctrl.cancel_flag.load(Ordering::Relaxed) || ctrl.pause_flag.load(Ordering::Relaxed) {
            return Ok(());
        }

        // 1. Acquire next Pending segment OR dynamically steal half of the largest remaining Active segment!
        let acquired = acquire_or_steal_segment(&shared_segments, is_range_mode).await;
        let Some(seg) = acquired else {
            return Ok(()); // All segments complete!
        };

        *seg.assigned_if_id.lock().unwrap() = Some(current_route.interface_id.clone());
        *seg.assigned_if_name.lock().unwrap() = Some(current_route.interface_name.clone());

        let mut attempt = 0usize;
        loop {
            if ctrl.cancel_flag.load(Ordering::Relaxed) || ctrl.pause_flag.load(Ordering::Relaxed) {
                *seg.status.lock().unwrap() = "Pending".into();
                return Ok(());
            }

            let res = download_single_segment(
                &current_route,
                &seg,
                &path_bytes,
                &ctrl,
                &url,
                &part_path,
                is_range_mode,
                &db,
            )
            .await;

            match res {
                Ok(()) => {
                    *seg.status.lock().unwrap() = "Completed".into();
                    seg.speed_bps.store(0, Ordering::Relaxed);
                    break;
                }
                Err(_)
                    if ctrl.pause_flag.load(Ordering::Relaxed)
                        || ctrl.cancel_flag.load(Ordering::Relaxed) =>
                {
                    *seg.status.lock().unwrap() = "Pending".into();
                    return Ok(());
                }
                Err(e) => {
                    attempt += 1;
                    seg.retries.fetch_add(1, Ordering::Relaxed);
                    if attempt >= 5 {
                        *seg.status.lock().unwrap() = "Failed".into();
                        return Err(format!(
                            "Segment failed after {attempt} retries across available interfaces: {e}"
                        ));
                    }
                    // Failover to another active network interface route immediately!
                    if !fallback_routes.is_empty() {
                        current_route =
                            fallback_routes[(worker_idx + attempt) % fallback_routes.len()].clone();
                        *seg.assigned_if_id.lock().unwrap() =
                            Some(current_route.interface_id.clone());
                        *seg.assigned_if_name.lock().unwrap() =
                            Some(current_route.interface_name.clone());
                    }
                    tokio::time::sleep(Duration::from_millis(400 * attempt as u64)).await;
                }
            }
        }
    }
}

async fn acquire_or_steal_segment(
    shared_segments: &Arc<Mutex<Vec<Arc<SharedSegmentState>>>>,
    is_range_mode: bool,
) -> Option<Arc<SharedSegmentState>> {
    let mut segs = shared_segments.lock().await;

    // First look for any Pending segment
    for s in segs.iter() {
        let mut st = s.status.lock().unwrap();
        if *st == "Pending" {
            *st = "Active".into();
            return Some(s.clone());
        }
    }

    if !is_range_mode || segs.len() >= 32 {
        return None;
    }

    // Dynamic Work-Stealing: Find active segment with largest remaining bytes >= 2 MiB and split it in half!
    let mut best_victim: Option<Arc<SharedSegmentState>> = None;
    let mut best_rem = 0u64;

    for s in segs.iter() {
        if *s.status.lock().unwrap() == "Active" {
            let cur_pos = s.start_byte + s.downloaded_bytes.load(Ordering::Relaxed);
            let end = s.end_byte.load(Ordering::Relaxed);
            if end > cur_pos {
                let rem = end - cur_pos + 1;
                if rem >= MIN_SPLITTABLE_REMAINING_BYTES && rem > best_rem {
                    best_rem = rem;
                    best_victim = Some(s.clone());
                }
            }
        }
    }

    if let Some(victim) = best_victim {
        let cur_pos = victim.start_byte + victim.downloaded_bytes.load(Ordering::Relaxed);
        let old_end = victim.end_byte.load(Ordering::Relaxed);
        if old_end > cur_pos + MIN_SPLITTABLE_REMAINING_BYTES / 2 {
            let mid = cur_pos + (old_end - cur_pos) / 2;
            victim.end_byte.store(mid, Ordering::Relaxed);

            let stolen = Arc::new(SharedSegmentState {
                start_byte: mid + 1,
                end_byte: AtomicU64::new(old_end),
                downloaded_bytes: AtomicU64::new(0),
                status: StdMutex::new("Active".into()),
                assigned_if_id: StdMutex::new(None),
                assigned_if_name: StdMutex::new(None),
                speed_bps: AtomicU64::new(0),
                retries: AtomicU64::new(0),
            });
            segs.push(stolen.clone());
            return Some(stolen);
        }
    }

    None
}

#[allow(clippy::too_many_arguments)]
async fn download_single_segment(
    route: &PathRoute,
    seg: &Arc<SharedSegmentState>,
    path_bytes: &Arc<StdMutex<HashMap<String, (String, InterfaceKind, u64, bool)>>>,
    ctrl: &Arc<ActiveTaskControl>,
    url: &str,
    part_path: &Path,
    is_range_mode: bool,
    db: &Arc<Database>,
) -> Result<(), String> {
    if ctrl.cancel_flag.load(Ordering::Relaxed) || ctrl.pause_flag.load(Ordering::Relaxed) {
        return Err("Interrupted".into());
    }

    let already = seg.downloaded_bytes.load(Ordering::Relaxed);
    let range_start = seg.start_byte + already;
    let initial_end = seg.end_byte.load(Ordering::Relaxed);

    if is_range_mode && initial_end > 0 && range_start > initial_end {
        return Ok(());
    }

    let mut req = route.client.get(url);
    let requested_range = if is_range_mode && initial_end > 0 {
        req = req.header(
            reqwest::header::RANGE,
            format!("bytes={range_start}-{initial_end}"),
        );
        true
    } else if already > 0 {
        // Single-segment resume: request bytes from the exact offset where we paused!
        if initial_end > 0 && range_start <= initial_end {
            req = req.header(
                reqwest::header::RANGE,
                format!("bytes={range_start}-{initial_end}"),
            );
        } else {
            req = req.header(reqwest::header::RANGE, format!("bytes={range_start}-"));
        }
        true
    } else {
        false
    };

    let send_res = tokio::select! {
        biased;
        _ = ctrl.stop_notify.notified() => return Err("Interrupted".into()),
        res = req.send() => res,
    };

    let resp = send_res
        .map_err(|e| format!("Network request failed on {}: {e}", route.interface_name))?;

    let status = resp.status();
    if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!(
            "HTTP {} from server on {}",
            status, route.interface_name
        ));
    }

    // Determine exact file write offset based on whether the server honored our Range header (206 Partial Content)
    let mut write_offset = if status == reqwest::StatusCode::PARTIAL_CONTENT {
        range_start
    } else if requested_range && range_start > 0 {
        if seg.start_byte > 0 {
            return Err(format!(
                "Server returned HTTP {} instead of 206 Partial Content for byte range starting at {}",
                status, range_start
            ));
        }
        // Single-stream server that does not support Range resume: restart from 0
        seg.downloaded_bytes.store(0, Ordering::Relaxed);
        0
    } else {
        seg.downloaded_bytes.store(0, Ordering::Relaxed);
        0
    };

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(part_path)
        .await
        .map_err(|e| format!("File open failed: {e}"))?;

    file.seek(SeekFrom::Start(write_offset))
        .await
        .map_err(|e| format!("File seek failed: {e}"))?;

    let mut stream = resp.bytes_stream();
    let mut window_bytes = 0u64;
    let mut window_start = Instant::now();

    loop {
        if ctrl.cancel_flag.load(Ordering::Relaxed) || ctrl.pause_flag.load(Ordering::Relaxed) {
            let _ = file.flush().await;
            let _ = file.sync_data().await;
            return Err("Interrupted".into());
        }

        let next_item = tokio::select! {
            biased;
            _ = ctrl.stop_notify.notified() => {
                let _ = file.flush().await;
                let _ = file.sync_data().await;
                return Err("Interrupted".into());
            }
            timed = tokio::time::timeout(Duration::from_secs(25), stream.next()) => {
                match timed {
                    Ok(opt) => opt,
                    Err(_) => {
                        let _ = file.flush().await;
                        return Err(format!("Stream read timed out on {}", route.interface_name));
                    }
                }
            }
        };

        let Some(chunk_res) = next_item else {
            break;
        };

        let chunk =
            chunk_res.map_err(|e| format!("Stream read error on {}: {e}", route.interface_name))?;
        if chunk.is_empty() {
            continue;
        }

        // Respect dynamic end_byte in case work-stealing shrank our segment while we were streaming!
        let current_end = seg.end_byte.load(Ordering::Relaxed);
        let to_write = if is_range_mode && current_end > 0 {
            if write_offset > current_end {
                break;
            }
            let max_rem = (current_end - write_offset + 1) as usize;
            chunk.len().min(max_rem)
        } else {
            chunk.len()
        };

        file.write_all(&chunk[..to_write])
            .await
            .map_err(|e| format!("Disk write error: {e}"))?;

        write_offset += to_write as u64;
        seg.downloaded_bytes
            .fetch_add(to_write as u64, Ordering::Relaxed);

        {
            let mut pmap = path_bytes.lock().unwrap();
            let entry = pmap.entry(route.interface_id.clone()).or_insert_with(|| {
                (
                    route.interface_name.clone(),
                    route.kind,
                    0,
                    route.via_tunnel,
                )
            });
            entry.2 += to_write as u64;
        }

        // Update per-segment instantaneous speed & enforce token-bucket speed limit
        window_bytes += to_write as u64;
        let elapsed = window_start.elapsed().as_secs_f64();
        if elapsed >= 0.25 {
            seg.speed_bps
                .store(((window_bytes as f64 * 8.0) / elapsed) as u64, Ordering::Relaxed);

            let task_limit = ctrl.speed_limit_bps.load(Ordering::Relaxed);
            let global_limit = db.get_settings().global_speed_limit_bps.unwrap_or(0);
            let effective_limit = match (task_limit, global_limit) {
                (0, 0) => 0,
                (a, 0) => a,
                (0, b) => b,
                (a, b) => a.min(b),
            };
            if effective_limit > 0 {
                let target_secs = (window_bytes as f64 * 8.0) / (effective_limit as f64 / 2.0);
                if target_secs > elapsed {
                    let sleep_dur = Duration::from_secs_f64((target_secs - elapsed).min(1.0));
                    tokio::select! {
                        biased;
                        _ = ctrl.stop_notify.notified() => {
                            let _ = file.flush().await;
                            let _ = file.sync_data().await;
                            return Err("Interrupted".into());
                        }
                        _ = tokio::time::sleep(sleep_dur) => {}
                    }
                }
            }

            window_bytes = 0;
            window_start = Instant::now();
        }

        if is_range_mode && current_end > 0 && write_offset > seg.end_byte.load(Ordering::Relaxed) {
            break;
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("File flush failed: {e}"))?;
    let _ = file.sync_data().await;
    Ok(())
}

async fn sum_downloaded_bytes(
    shared_segments: &Arc<Mutex<Vec<Arc<SharedSegmentState>>>>,
) -> u64 {
    let segs = shared_segments.lock().await;
    segs.iter()
        .map(|s| s.downloaded_bytes.load(Ordering::Relaxed))
        .sum()
}

fn build_weighted_route_ring(routes: &[PathRoute]) -> Vec<PathRoute> {
    let mut ring = Vec::new();
    for r in routes {
        let slots = ((r.weight_pct / 10.0).round() as usize).clamp(1, 10);
        for _ in 0..slots {
            ring.push(r.clone());
        }
    }
    if ring.is_empty() && !routes.is_empty() {
        ring.push(routes[0].clone());
    }
    ring
}

pub fn build_http_client(socks_port: Option<u16>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent("NetFusion/1.0 (Windows NT 10.0; Win64; x64; Multipath Bonding Engine)")
        .connect_timeout(Duration::from_secs(12))
        .tcp_nodelay(true);

    if let Some(port) = socks_port {
        let proxy = reqwest::Proxy::all(format!("socks5h://127.0.0.1:{port}"))
            .map_err(|e| format!("Proxy config error: {e}"))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|e| format!("HTTP client init error: {e}"))
}

async fn compute_file_checksum(path: &Path, algorithm: &str) -> Result<String, String> {
    let mut file = File::open(path)
        .await
        .map_err(|e| format!("Cannot open file for checksum verification: {e}"))?;
    let mut buf = vec![0u8; 256 * 1024];

    if algorithm.eq_ignore_ascii_case("MD5") {
        let mut hasher = Md5::new();
        loop {
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| format!("Checksum read error: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hex::encode(hasher.finalize()))
    } else {
        let mut hasher = Sha256::new();
        loop {
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| format!("Checksum read error: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hex::encode(hasher.finalize()))
    }
}

