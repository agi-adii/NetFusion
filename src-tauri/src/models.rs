use netfusion_protocol::scheduler::BondingPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterfaceKind {
    WiFi,
    Ethernet,
    UsbTethering,
    Cellular,
    Virtual,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterfaceStatus {
    Connected,
    Disconnected,
    Unstable,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterfaceInfo {
    pub id: String,
    pub if_index: u32,
    pub name: String,
    pub description: String,
    pub kind: InterfaceKind,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub gateway: Option<String>,
    pub mac_address: String,
    pub status: InterfaceStatus,
    pub enabled_for_bonding: bool,
    pub is_primary_failover: bool,
    pub link_speed_bps: u64,
    pub download_bps: u64,
    pub upload_bps: u64,
    pub latency_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub packet_loss_pct: f64,
    pub stability_score: f64,
    pub rx_bytes_session: u64,
    pub tx_bytes_session: u64,
    pub rx_bytes_today: u64,
    pub tx_bytes_today: u64,
    pub allocated_weight_pct: f64,
    pub custom_weight: u32,
    pub subflow_connected: bool,
    pub local_proxy_port: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TunnelStatus {
    Disconnected,
    Connecting,
    Connected,
    Degraded,
    AuthFailed,
    ServerUnreachable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondingStateSnapshot {
    pub mode: BondingPolicy,
    pub tunnel_enabled: bool,
    pub tunnel_status: TunnelStatus,
    pub server_endpoint: String,
    pub active_subflows: usize,
    pub local_socks_port: Option<u16>,
    pub tunnel_rtt_ms: Option<f64>,
    pub reassembly_queue_depth: usize,
    pub total_bonded_rx_bytes: u64,
    pub total_bonded_tx_bytes: u64,
    pub status_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTelemetrySnapshot {
    pub timestamp_ms: u64,
    pub total_download_bps: u64,
    pub total_upload_bps: u64,
    pub total_link_capacity_bps: u64,
    pub active_connections_count: usize,
    pub average_latency_ms: Option<f64>,
    pub overall_stability_pct: f64,
    pub downloaded_today_bytes: u64,
    pub uploaded_today_bytes: u64,
    pub interfaces: Vec<NetworkInterfaceInfo>,
    pub bonding: BondingStateSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadStatus {
    Queued,
    Scheduled,
    Probing,
    Downloading,
    Paused,
    Verifying,
    Merging,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Scheduled => "Scheduled",
            Self::Probing => "Probing",
            Self::Downloading => "Downloading",
            Self::Paused => "Paused",
            Self::Verifying => "Verifying",
            Self::Merging => "Merging",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "Queued" => Self::Queued,
            "Scheduled" => Self::Scheduled,
            "Probing" => Self::Probing,
            "Downloading" => Self::Downloading,
            "Paused" => Self::Paused,
            "Verifying" => Self::Verifying,
            "Merging" => Self::Merging,
            "Completed" => Self::Completed,
            "Failed" => Self::Failed,
            "Cancelled" => Self::Cancelled,
            _ => Self::Paused,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl DownloadPriority {
    pub fn rank(&self) -> i32 {
        match self {
            Self::Critical => 4,
            Self::High => 3,
            Self::Normal => 2,
            Self::Low => 1,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Normal => "Normal",
            Self::High => "High",
            Self::Critical => "Critical",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "Low" => Self::Low,
            "High" => Self::High,
            "Critical" => Self::Critical,
            _ => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSegmentInfo {
    pub index: usize,
    pub start_byte: u64,
    pub end_byte: u64,
    pub downloaded_bytes: u64,
    pub status: String, // "Pending" | "Active" | "Completed" | "Failed"
    pub assigned_interface_id: Option<String>,
    pub assigned_interface_name: Option<String>,
    pub speed_bps: u64,
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivePathContribution {
    pub interface_id: String,
    pub interface_name: String,
    pub kind: InterfaceKind,
    pub speed_bps: u64,
    pub bytes_contributed: u64,
    pub via_tunnel: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub save_dir: String,
    pub target_path: String,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: DownloadStatus,
    pub priority: DownloadPriority,
    pub supports_range: bool,
    pub speed_bps: u64,
    pub avg_speed_bps: u64,
    pub eta_seconds: Option<u64>,
    pub speed_limit_bps: Option<u64>,
    pub scheduled_at_ms: Option<u64>,
    pub expected_checksum: Option<String>,
    pub checksum_algorithm: Option<String>,
    pub computed_checksum: Option<String>,
    pub checksum_verified: Option<bool>,
    pub error_message: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub created_at_ms: u64,
    pub completed_at_ms: Option<u64>,
    pub used_bonding_tunnel: bool,
    pub segments: Vec<DownloadSegmentInfo>,
    pub active_paths: Vec<ActivePathContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddDownloadRequest {
    pub url: String,
    pub file_name: Option<String>,
    pub save_dir: Option<String>,
    pub priority: DownloadPriority,
    pub max_segments: Option<usize>,
    pub speed_limit_bps: Option<u64>,
    pub scheduled_at_ms: Option<u64>,
    pub expected_checksum: Option<String>,
    pub checksum_algorithm: Option<String>,
    pub start_immediately: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlProbeResult {
    pub valid: bool,
    pub normalized_url: String,
    pub suggested_file_name: String,
    pub total_bytes: Option<u64>,
    pub supports_range: bool,
    pub content_type: Option<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionHistoryEvent {
    pub id: i64,
    pub timestamp_ms: u64,
    pub interface_id: String,
    pub interface_name: String,
    pub kind: InterfaceKind,
    pub event_type: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedTestProgress {
    pub phase: String, // "Idle" | "Ping" | "Download" | "Upload" | "Complete" | "Error"
    pub progress_pct: f64,
    pub current_bps: u64,
    pub ping_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub packet_loss_pct: f64,
    pub download_bps: u64,
    pub upload_bps: u64,
    pub target_label: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedTestRecord {
    pub id: i64,
    pub timestamp_ms: u64,
    pub mode_label: String,
    pub interface_name: String,
    pub server_label: String,
    pub download_bps: u64,
    pub upload_bps: u64,
    pub ping_ms: f64,
    pub jitter_ms: f64,
    pub packet_loss_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub default_download_dir: String,
    pub max_concurrent_downloads: usize,
    pub default_segments_per_download: usize,
    pub adaptive_concurrency: bool,
    pub global_speed_limit_bps: Option<u64>,
    pub auto_retry_downloads: bool,
    pub max_retries: u32,
    pub bonding_mode: BondingPolicy,
    pub bonding_tunnel_enabled: bool,
    pub bonding_server_endpoint: String,
    pub bonding_auth_token_set: bool,
    pub bonding_auth_token_preview: String,
    pub enable_local_socks_proxy: bool,
    pub local_socks_port: u16,
    pub launch_at_startup: bool,
    pub minimize_to_tray: bool,
    pub run_in_background: bool,
    pub notify_on_complete: bool,
    pub theme: String,
    pub speedtest_endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSettingsPayload {
    pub default_download_dir: String,
    pub max_concurrent_downloads: usize,
    pub default_segments_per_download: usize,
    pub adaptive_concurrency: bool,
    pub global_speed_limit_bps: Option<u64>,
    pub auto_retry_downloads: bool,
    pub max_retries: u32,
    pub bonding_mode: BondingPolicy,
    pub bonding_tunnel_enabled: bool,
    pub bonding_server_endpoint: String,
    pub new_bonding_auth_token: Option<String>,
    pub enable_local_socks_proxy: bool,
    pub local_socks_port: u16,
    pub launch_at_startup: bool,
    pub minimize_to_tray: bool,
    pub run_in_background: bool,
    pub notify_on_complete: bool,
    pub theme: String,
    pub speedtest_endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: i64,
    pub timestamp_ms: u64,
    pub level: String,
    pub category: String,
    pub message: String,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

