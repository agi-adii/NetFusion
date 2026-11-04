use crate::models::{
    now_ms, AppSettings, ConnectionHistoryEvent, DownloadItem, DownloadPriority,
    DownloadSegmentInfo, DownloadStatus, InterfaceKind, LogEntry, SpeedTestRecord,
    UpdateSettingsPayload,
};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use hkdf::Hkdf;
use netfusion_protocol::scheduler::BondingPolicy;
use rand::RngCore;
use rusqlite::{params, Connection};
use sha2::Sha256;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
    vault_key: [u8; 32],
}

impl Database {
    pub fn open(db_path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(&db_path).map_err(|e| format!("SQLite open error: {e}"))?;
        let _ = conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;");

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS settings_kv (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS interface_prefs (
                interface_id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL DEFAULT 1,
                custom_weight INTEGER NOT NULL DEFAULT 50,
                is_primary INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS interface_daily_usage (
                day_key TEXT NOT NULL,
                interface_id TEXT NOT NULL,
                rx_bytes INTEGER NOT NULL DEFAULT 0,
                tx_bytes INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (day_key, interface_id)
            );

            CREATE TABLE IF NOT EXISTS downloads (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                file_name TEXT NOT NULL,
                save_dir TEXT NOT NULL,
                target_path TEXT NOT NULL,
                total_bytes INTEGER,
                downloaded_bytes INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                priority TEXT NOT NULL,
                supports_range INTEGER NOT NULL DEFAULT 0,
                avg_speed_bps INTEGER NOT NULL DEFAULT 0,
                speed_limit_bps INTEGER,
                scheduled_at_ms INTEGER,
                expected_checksum TEXT,
                checksum_algorithm TEXT,
                computed_checksum TEXT,
                checksum_verified INTEGER,
                error_message TEXT,
                retry_count INTEGER NOT NULL DEFAULT 0,
                max_retries INTEGER NOT NULL DEFAULT 5,
                created_at_ms INTEGER NOT NULL,
                completed_at_ms INTEGER,
                elapsed_ms INTEGER NOT NULL DEFAULT 0,
                used_bonding_tunnel INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS download_segments (
                download_id TEXT NOT NULL,
                seg_index INTEGER NOT NULL,
                start_byte INTEGER NOT NULL,
                end_byte INTEGER NOT NULL,
                downloaded_bytes INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                retries INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (download_id, seg_index)
            );

            CREATE TABLE IF NOT EXISTS connection_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                interface_id TEXT NOT NULL,
                interface_name TEXT NOT NULL,
                kind TEXT NOT NULL,
                event_type TEXT NOT NULL,
                details TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS speed_tests (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                mode_label TEXT NOT NULL,
                interface_name TEXT NOT NULL,
                server_label TEXT NOT NULL,
                download_bps INTEGER NOT NULL,
                upload_bps INTEGER NOT NULL,
                ping_ms REAL NOT NULL,
                jitter_ms REAL NOT NULL,
                packet_loss_pct REAL NOT NULL
            );

            CREATE TABLE IF NOT EXISTS logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                level TEXT NOT NULL,
                category TEXT NOT NULL,
                message TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| format!("SQLite schema init error: {e}"))?;

        // Seamless schema migration for existing databases created prior to elapsed_ms tracking
        let _ = conn.execute(
            "ALTER TABLE downloads ADD COLUMN elapsed_ms INTEGER NOT NULL DEFAULT 0",
            [],
        );

        let salt_hex: String = match conn.query_row(
            "SELECT value FROM settings_kv WHERE key = 'vault_salt'",
            [],
            |r| r.get(0),
        ) {
            Ok(s) => s,
            Err(_) => {
                let mut salt = [0u8; 16];
                rand::thread_rng().fill_bytes(&mut salt);
                let s = hex::encode(salt);
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('vault_salt', ?1)",
                    params![s],
                );
                s
            }
        };

        let vault_key = derive_machine_vault_key(&salt_hex);
        let db = Self {
            conn: Mutex::new(conn),
            vault_key,
        };

        // Ensure default encrypted PSK exists (never stored in plaintext)
        if db.get_raw_kv("bonding_psk_cipher").is_none() {
            let _ = db.store_bonding_psk("netfusion-default-change-me-in-prod");
        }

        Ok(db)
    }

    fn get_raw_kv(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT value FROM settings_kv WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .ok()
    }

    fn set_raw_kv(&self, key: &str, value: &str) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO settings_kv (key, value) VALUES (?1, ?2)",
                params![key, value],
            );
        }
    }

    pub fn store_bonding_psk(&self, plaintext_psk: &str) -> Result<(), String> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.vault_key));
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext_psk.as_bytes())
            .map_err(|_| "Failed to encrypt PSK".to_string())?;
        let mut combined = Vec::with_capacity(12 + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);
        self.set_raw_kv("bonding_psk_cipher", &hex::encode(combined));
        Ok(())
    }

    pub fn load_bonding_psk(&self) -> String {
        if let Some(hex_blob) = self.get_raw_kv("bonding_psk_cipher") {
            if let Ok(raw) = hex::decode(hex_blob) {
                if raw.len() > 12 {
                    let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.vault_key));
                    if let Ok(pt) =
                        cipher.decrypt(Nonce::from_slice(&raw[..12]), &raw[12..])
                    {
                        if let Ok(s) = String::from_utf8(pt) {
                            return s;
                        }
                    }
                }
            }
        }
        "netfusion-default-change-me-in-prod".to_string()
    }

    pub fn get_settings(&self) -> AppSettings {
        let default_dl_dir = dirs::download_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join("NetFusion")
            .to_string_lossy()
            .to_string();

        if let Some(json_str) = self.get_raw_kv("app_settings_json") {
            if let Ok(mut parsed) = serde_json::from_str::<AppSettings>(&json_str) {
                if parsed.default_download_dir.is_empty() {
                    parsed.default_download_dir = default_dl_dir;
                }
                if parsed.bonding_server_endpoint.trim().is_empty() {
                    parsed.bonding_server_endpoint = "127.0.0.1:9443".to_string();
                }
                if parsed.bonding_server_endpoint.starts_with("127.0.0.1:") {
                    parsed.bonding_tunnel_enabled = true;
                }
                parsed.bonding_auth_token_set = self.get_raw_kv("bonding_psk_cipher").is_some();
                parsed.bonding_auth_token_preview = "AEAD Encrypted in Vault".to_string();
                return parsed;
            }
        }

        AppSettings {
            default_download_dir: default_dl_dir,
            max_concurrent_downloads: 4,
            default_segments_per_download: 8,
            adaptive_concurrency: true,
            global_speed_limit_bps: None,
            auto_retry_downloads: true,
            max_retries: 5,
            bonding_mode: BondingPolicy::Smart,
            bonding_tunnel_enabled: true,
            bonding_server_endpoint: "127.0.0.1:9443".to_string(),
            bonding_auth_token_set: true,
            bonding_auth_token_preview: "AEAD Encrypted in Vault".to_string(),
            enable_local_socks_proxy: true,
            local_socks_port: 10808,
            launch_at_startup: false,
            minimize_to_tray: true,
            run_in_background: true,
            notify_on_complete: true,
            theme: "dark".to_string(),
            speedtest_endpoint: "https://speed.cloudflare.com".to_string(),
        }
    }

    pub fn save_settings(&self, payload: &UpdateSettingsPayload) -> Result<AppSettings, String> {
        if let Some(ref new_psk) = payload.new_bonding_auth_token {
            if !new_psk.trim().is_empty() {
                self.store_bonding_psk(new_psk.trim())?;
            }
        }

        let updated = AppSettings {
            default_download_dir: payload.default_download_dir.clone(),
            max_concurrent_downloads: payload.max_concurrent_downloads.clamp(1, 16),
            default_segments_per_download: payload.default_segments_per_download.clamp(1, 32),
            adaptive_concurrency: payload.adaptive_concurrency,
            global_speed_limit_bps: payload.global_speed_limit_bps,
            auto_retry_downloads: payload.auto_retry_downloads,
            max_retries: payload.max_retries.min(25),
            bonding_mode: payload.bonding_mode,
            bonding_tunnel_enabled: payload.bonding_tunnel_enabled,
            bonding_server_endpoint: payload.bonding_server_endpoint.clone(),
            bonding_auth_token_set: true,
            bonding_auth_token_preview: "AEAD Encrypted in Vault".to_string(),
            enable_local_socks_proxy: payload.enable_local_socks_proxy,
            local_socks_port: payload.local_socks_port,
            launch_at_startup: payload.launch_at_startup,
            minimize_to_tray: payload.minimize_to_tray,
            run_in_background: payload.run_in_background,
            notify_on_complete: payload.notify_on_complete,
            theme: payload.theme.clone(),
            speedtest_endpoint: payload.speedtest_endpoint.clone(),
        };

        let json_str = serde_json::to_string(&updated).map_err(|e| e.to_string())?;
        self.set_raw_kv("app_settings_json", &json_str);
        Ok(updated)
    }

    pub fn set_bonding_mode_only(&self, mode: BondingPolicy) {
        let mut s = self.get_settings();
        s.bonding_mode = mode;
        if let Ok(j) = serde_json::to_string(&s) {
            self.set_raw_kv("app_settings_json", &j);
        }
    }

    pub fn load_interface_prefs(&self) -> HashMap<String, (bool, u32, bool)> {
        let mut out = HashMap::new();
        if let Ok(conn) = self.conn.lock() {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT interface_id, enabled, custom_weight, is_primary FROM interface_prefs",
            ) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)? != 0,
                        r.get::<_, u32>(2)?,
                        r.get::<_, i64>(3)? != 0,
                    ))
                }) {
                    for row in rows.flatten() {
                        out.insert(row.0, (row.1, row.2, row.3));
                    }
                }
            }
        }
        out
    }

    pub fn upsert_interface_pref(
        &self,
        interface_id: &str,
        enabled: bool,
        custom_weight: u32,
        is_primary: bool,
    ) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT INTO interface_prefs (interface_id, enabled, custom_weight, is_primary)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(interface_id) DO UPDATE SET
                   enabled = excluded.enabled,
                   custom_weight = excluded.custom_weight,
                   is_primary = excluded.is_primary",
                params![
                    interface_id,
                    if enabled { 1 } else { 0 },
                    custom_weight,
                    if is_primary { 1 } else { 0 }
                ],
            );
        }
    }

    pub fn add_daily_usage(&self, day_key: &str, interface_id: &str, rx_delta: u64, tx_delta: u64) {
        if rx_delta == 0 && tx_delta == 0 {
            return;
        }
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT INTO interface_daily_usage (day_key, interface_id, rx_bytes, tx_bytes)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(day_key, interface_id) DO UPDATE SET
                   rx_bytes = rx_bytes + excluded.rx_bytes,
                   tx_bytes = tx_bytes + excluded.tx_bytes",
                params![day_key, interface_id, rx_delta as i64, tx_delta as i64],
            );
        }
    }

    pub fn load_daily_usage(&self, day_key: &str) -> HashMap<String, (u64, u64)> {
        let mut map = HashMap::new();
        if let Ok(conn) = self.conn.lock() {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT interface_id, rx_bytes, tx_bytes FROM interface_daily_usage WHERE day_key = ?1",
            ) {
                if let Ok(rows) = stmt.query_map(params![day_key], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?.max(0) as u64,
                        r.get::<_, i64>(2)?.max(0) as u64,
                    ))
                }) {
                    for row in rows.flatten() {
                        map.insert(row.0, (row.1, row.2));
                    }
                }
            }
        }
        map
    }

    pub fn insert_connection_event(
        &self,
        interface_id: &str,
        interface_name: &str,
        kind: InterfaceKind,
        event_type: &str,
        details: &str,
    ) -> ConnectionHistoryEvent {
        let ts = now_ms();
        let kind_str = serde_json::to_string(&kind).unwrap_or_else(|_| "\"Other\"".into());
        let mut id = 0i64;
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT INTO connection_events (timestamp_ms, interface_id, interface_name, kind, event_type, details)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![ts as i64, interface_id, interface_name, kind_str, event_type, details],
            );
            id = conn.last_insert_rowid();
        }
        ConnectionHistoryEvent {
            id,
            timestamp_ms: ts,
            interface_id: interface_id.to_string(),
            interface_name: interface_name.to_string(),
            kind,
            event_type: event_type.to_string(),
            details: details.to_string(),
        }
    }

    pub fn list_connection_events(&self, limit: usize) -> Vec<ConnectionHistoryEvent> {
        let mut out = Vec::new();
        if let Ok(conn) = self.conn.lock() {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT id, timestamp_ms, interface_id, interface_name, kind, event_type, details
                 FROM connection_events ORDER BY id DESC LIMIT ?1",
            ) {
                if let Ok(rows) = stmt.query_map(params![limit as i64], |r| {
                    let kind_json: String = r.get(4)?;
                    let kind: InterfaceKind =
                        serde_json::from_str(&kind_json).unwrap_or(InterfaceKind::Other);
                    Ok(ConnectionHistoryEvent {
                        id: r.get(0)?,
                        timestamp_ms: r.get::<_, i64>(1)? as u64,
                        interface_id: r.get(2)?,
                        interface_name: r.get(3)?,
                        kind,
                        event_type: r.get(5)?,
                        details: r.get(6)?,
                    })
                }) {
                    for row in rows.flatten() {
                        out.push(row);
                    }
                }
            }
        }
        out
    }

    pub fn save_download(&self, item: &DownloadItem) {
        if let Ok(conn) = self.conn.lock() {
            let verified_int: Option<i64> = item.checksum_verified.map(|b| if b { 1 } else { 0 });
            let _ = conn.execute(
                "INSERT OR REPLACE INTO downloads (
                    id, url, file_name, save_dir, target_path, total_bytes, downloaded_bytes,
                    status, priority, supports_range, avg_speed_bps, speed_limit_bps,
                    scheduled_at_ms, expected_checksum, checksum_algorithm, computed_checksum,
                    checksum_verified, error_message, retry_count, max_retries,
                    created_at_ms, completed_at_ms, elapsed_ms, used_bonding_tunnel
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                    ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15, ?16,
                    ?17, ?18, ?19, ?20,
                    ?21, ?22, ?23, ?24
                )",
                params![
                    item.id,
                    item.url,
                    item.file_name,
                    item.save_dir,
                    item.target_path,
                    item.total_bytes.map(|v| v as i64),
                    item.downloaded_bytes as i64,
                    item.status.as_str(),
                    item.priority.as_str(),
                    if item.supports_range { 1 } else { 0 },
                    item.avg_speed_bps as i64,
                    item.speed_limit_bps.map(|v| v as i64),
                    item.scheduled_at_ms.map(|v| v as i64),
                    item.expected_checksum,
                    item.checksum_algorithm,
                    item.computed_checksum,
                    verified_int,
                    item.error_message,
                    item.retry_count,
                    item.max_retries,
                    item.created_at_ms as i64,
                    item.completed_at_ms.map(|v| v as i64),
                    item.elapsed_ms as i64,
                    if item.used_bonding_tunnel { 1 } else { 0 },
                ],
            );

            let _ = conn.execute(
                "DELETE FROM download_segments WHERE download_id = ?1",
                params![item.id],
            );
            for seg in &item.segments {
                let _ = conn.execute(
                    "INSERT INTO download_segments (
                        download_id, seg_index, start_byte, end_byte, downloaded_bytes, status, retries
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        item.id,
                        seg.index as i64,
                        seg.start_byte as i64,
                        seg.end_byte as i64,
                        seg.downloaded_bytes as i64,
                        seg.status,
                        seg.retries,
                    ],
                );
            }
        }
    }

    pub fn delete_download(&self, download_id: &str) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "DELETE FROM download_segments WHERE download_id = ?1",
                params![download_id],
            );
            let _ = conn.execute("DELETE FROM downloads WHERE id = ?1", params![download_id]);
        }
    }

    pub fn load_all_downloads(&self) -> Vec<DownloadItem> {
        let mut items = Vec::new();
        let Ok(conn) = self.conn.lock() else {
            return items;
        };

        let mut seg_map: HashMap<String, Vec<DownloadSegmentInfo>> = HashMap::new();
        if let Ok(mut s_stmt) = conn.prepare(
            "SELECT download_id, seg_index, start_byte, end_byte, downloaded_bytes, status, retries
             FROM download_segments ORDER BY seg_index ASC",
        ) {
            if let Ok(rows) = s_stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    DownloadSegmentInfo {
                        index: r.get::<_, i64>(1)? as usize,
                        start_byte: r.get::<_, i64>(2)? as u64,
                        end_byte: r.get::<_, i64>(3)? as u64,
                        downloaded_bytes: r.get::<_, i64>(4)? as u64,
                        status: r.get(5)?,
                        assigned_interface_id: None,
                        assigned_interface_name: None,
                        speed_bps: 0,
                        retries: r.get(6)?,
                    },
                ))
            }) {
                for (dl_id, mut seg) in rows.flatten() {
                    if seg.status == "Active" {
                        seg.status = "Pending".to_string();
                    }
                    seg_map.entry(dl_id).or_default().push(seg);
                }
            }
        }

        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, url, file_name, save_dir, target_path, total_bytes, downloaded_bytes,
                    status, priority, supports_range, avg_speed_bps, speed_limit_bps,
                    scheduled_at_ms, expected_checksum, checksum_algorithm, computed_checksum,
                    checksum_verified, error_message, retry_count, max_retries,
                    created_at_ms, completed_at_ms, used_bonding_tunnel, elapsed_ms
             FROM downloads ORDER BY created_at_ms DESC",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                let id: String = r.get(0)?;
                let mut status = DownloadStatus::from_str(&r.get::<_, String>(7)?);
                // Automatic restart recovery: active tasks become Queued so engine resumes them safely
                if matches!(
                    status,
                    DownloadStatus::Downloading
                        | DownloadStatus::Probing
                        | DownloadStatus::Verifying
                        | DownloadStatus::Merging
                ) {
                    status = DownloadStatus::Queued;
                }
                let verified_opt: Option<i64> = r.get(16)?;
                let created_at_ms = r.get::<_, i64>(20)?.max(0) as u64;
                let completed_at_ms = r.get::<_, Option<i64>>(21)?.map(|v| v.max(0) as u64);
                let mut elapsed_ms = r.get::<_, Option<i64>>(23)?.unwrap_or(0).max(0) as u64;
                if elapsed_ms == 0 {
                    if let Some(comp) = completed_at_ms {
                        if comp > created_at_ms {
                            elapsed_ms = comp - created_at_ms;
                        }
                    }
                }
                Ok(DownloadItem {
                    id: id.clone(),
                    url: r.get(1)?,
                    file_name: r.get(2)?,
                    save_dir: r.get(3)?,
                    target_path: r.get(4)?,
                    total_bytes: r.get::<_, Option<i64>>(5)?.map(|v| v as u64),
                    downloaded_bytes: r.get::<_, i64>(6)?.max(0) as u64,
                    status,
                    priority: DownloadPriority::from_str(&r.get::<_, String>(8)?),
                    supports_range: r.get::<_, i64>(9)? != 0,
                    speed_bps: 0,
                    avg_speed_bps: r.get::<_, i64>(10)?.max(0) as u64,
                    eta_seconds: None,
                    speed_limit_bps: r.get::<_, Option<i64>>(11)?.map(|v| v as u64),
                    scheduled_at_ms: r.get::<_, Option<i64>>(12)?.map(|v| v as u64),
                    expected_checksum: r.get(13)?,
                    checksum_algorithm: r.get(14)?,
                    computed_checksum: r.get(15)?,
                    checksum_verified: verified_opt.map(|v| v != 0),
                    error_message: r.get(17)?,
                    retry_count: r.get(18)?,
                    max_retries: r.get(19)?,
                    created_at_ms,
                    completed_at_ms,
                    elapsed_ms,
                    used_bonding_tunnel: r.get::<_, i64>(22)? != 0,
                    segments: seg_map.remove(&id).unwrap_or_default(),
                    active_paths: Vec::new(),
                })
            }) {
                for item in rows.flatten() {
                    items.push(item);
                }
            }
        }
        items
    }

    pub fn insert_speed_test(&self, rec: &SpeedTestRecord) -> SpeedTestRecord {
        let mut id = 0i64;
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT INTO speed_tests (
                    timestamp_ms, mode_label, interface_name, server_label,
                    download_bps, upload_bps, ping_ms, jitter_ms, packet_loss_pct
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    rec.timestamp_ms as i64,
                    rec.mode_label,
                    rec.interface_name,
                    rec.server_label,
                    rec.download_bps as i64,
                    rec.upload_bps as i64,
                    rec.ping_ms,
                    rec.jitter_ms,
                    rec.packet_loss_pct,
                ],
            );
            id = conn.last_insert_rowid();
        }
        let mut saved = rec.clone();
        saved.id = id;
        saved
    }

    pub fn list_speed_tests(&self, limit: usize) -> Vec<SpeedTestRecord> {
        let mut out = Vec::new();
        if let Ok(conn) = self.conn.lock() {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT id, timestamp_ms, mode_label, interface_name, server_label,
                        download_bps, upload_bps, ping_ms, jitter_ms, packet_loss_pct
                 FROM speed_tests ORDER BY id DESC LIMIT ?1",
            ) {
                if let Ok(rows) = stmt.query_map(params![limit as i64], |r| {
                    Ok(SpeedTestRecord {
                        id: r.get(0)?,
                        timestamp_ms: r.get::<_, i64>(1)? as u64,
                        mode_label: r.get(2)?,
                        interface_name: r.get(3)?,
                        server_label: r.get(4)?,
                        download_bps: r.get::<_, i64>(5)? as u64,
                        upload_bps: r.get::<_, i64>(6)? as u64,
                        ping_ms: r.get(7)?,
                        jitter_ms: r.get(8)?,
                        packet_loss_pct: r.get(9)?,
                    })
                }) {
                    for row in rows.flatten() {
                        out.push(row);
                    }
                }
            }
        }
        out
    }

    pub fn insert_log(&self, level: &str, category: &str, message: &str) -> LogEntry {
        let ts = now_ms();
        let mut id = 0i64;
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT INTO logs (timestamp_ms, level, category, message) VALUES (?1, ?2, ?3, ?4)",
                params![ts as i64, level, category, message],
            );
            id = conn.last_insert_rowid();
            // Keep logs table trimmed to 2000 rows
            let _ = conn.execute(
                "DELETE FROM logs WHERE id NOT IN (SELECT id FROM logs ORDER BY id DESC LIMIT 2000)",
                [],
            );
        }
        LogEntry {
            id,
            timestamp_ms: ts,
            level: level.to_string(),
            category: category.to_string(),
            message: message.to_string(),
        }
    }

    pub fn list_logs(&self, limit: usize) -> Vec<LogEntry> {
        let mut out = Vec::new();
        if let Ok(conn) = self.conn.lock() {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT id, timestamp_ms, level, category, message FROM logs ORDER BY id DESC LIMIT ?1",
            ) {
                if let Ok(rows) = stmt.query_map(params![limit as i64], |r| {
                    Ok(LogEntry {
                        id: r.get(0)?,
                        timestamp_ms: r.get::<_, i64>(1)? as u64,
                        level: r.get(2)?,
                        category: r.get(3)?,
                        message: r.get(4)?,
                    })
                }) {
                    for row in rows.flatten() {
                        out.push(row);
                    }
                }
            }
        }
        out
    }

    pub fn clear_logs(&self) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute("DELETE FROM logs", []);
        }
    }
}

fn derive_machine_vault_key(salt_hex: &str) -> [u8; 32] {
    let comp = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "WIN-HOST".into());
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "WIN-USER".into());
    let ikm = format!("NETFUSION-VAULT-V1:{comp}:{user}");
    let hk = Hkdf::<Sha256>::new(Some(salt_hex.as_bytes()), ikm.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"NETFUSION-PSK-AT-REST", &mut key).unwrap();
    key
}

