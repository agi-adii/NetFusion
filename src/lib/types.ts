export type InterfaceKind = "WiFi" | "Ethernet" | "UsbTethering" | "Cellular" | "Virtual" | "Other";

export type InterfaceStatus = "Connected" | "Disconnected" | "Unstable" | "Disabled";

export type BondingMode =
  | "Smart"
  | "MaxSpeed"
  | "Balanced"
  | "Failover"
  | "Custom";

export interface NetworkInterfaceInfo {
  id: string;
  if_index: number;
  name: string;
  slot_label?: string;
  description: string;
  kind: InterfaceKind;
  ipv4: string | null;
  ipv6: string | null;
  gateway: string | null;
  mac_address: string;
  status: InterfaceStatus;
  enabled_for_bonding: boolean;
  is_primary_failover: boolean;
  link_speed_bps: number;
  download_bps: number;
  upload_bps: number;
  peak_download_bps?: number;
  peak_upload_bps?: number;
  latency_ms: number | null;
  jitter_ms: number | null;
  packet_loss_pct: number;
  stability_score: number;
  rx_bytes_session: number;
  tx_bytes_session: number;
  rx_bytes_today: number;
  tx_bytes_today: number;
  allocated_weight_pct: number;
  custom_weight: number;
  subflow_connected: boolean;
  local_proxy_port: number | null;
  wifi_ssid?: string | null;
  wifi_bssid?: string | null;
  wifi_signal_pct?: number | null;
  wifi_radio_type?: string | null;
  wifi_profile?: string | null;
}

export interface WifiNetworkScanItem {
  ssid: string;
  bssid: string;
  signal_pct: number;
  radio_type: string;
  authentication: string;
  channel: string;
  connected: boolean;
  has_saved_profile: boolean;
}

export interface BandwidthSample {
  timestamp_ms: number;
  time_label: string;
  total_download_bps: number;
  total_upload_bps: number;
  wifi_download_bps: number;
  ethernet_download_bps: number;
  usb_download_bps: number;
  other_download_bps: number;
  per_interface: Record<string, { download_bps: number; upload_bps: number; latency_ms: number | null }>;
}

export type TunnelStatus =
  | "Disconnected"
  | "Connecting"
  | "Connected"
  | "Degraded"
  | "AuthFailed"
  | "ServerUnreachable";

export interface BondingStateSnapshot {
  bonding_active?: boolean;
  auto_bonding?: boolean;
  adaptive_load_balancing?: boolean;
  virtual_gateway_active?: boolean;
  system_proxy_routed?: boolean;
  virtual_adapter_detected?: string | null;
  mode: BondingMode;
  tunnel_enabled: boolean;
  tunnel_status: TunnelStatus;
  server_endpoint: string;
  active_subflows: number;
  local_socks_port: number | null;
  tunnel_rtt_ms: number | null;
  reassembly_queue_depth: number;
  total_bonded_rx_bytes: number;
  total_bonded_tx_bytes: number;
  status_message: string;
}

export interface NetworkTelemetrySnapshot {
  timestamp_ms: number;
  total_download_bps: number;
  total_upload_bps: number;
  total_link_capacity_bps: number;
  active_connections_count: number;
  average_latency_ms: number | null;
  average_packet_loss_pct?: number;
  overall_stability_pct: number;
  downloaded_today_bytes: number;
  uploaded_today_bytes: number;
  interfaces: NetworkInterfaceInfo[];
  bonding: BondingStateSnapshot;
}

export type DownloadStatus =
  | "Queued"
  | "Scheduled"
  | "Probing"
  | "Downloading"
  | "Paused"
  | "Verifying"
  | "Merging"
  | "Completed"
  | "Failed"
  | "Cancelled";

export type DownloadPriority = "Low" | "Normal" | "High" | "Critical";

export interface DownloadSegmentInfo {
  index: number;
  start_byte: number;
  end_byte: number;
  downloaded_bytes: number;
  status: "Pending" | "Active" | "Completed" | "Failed";
  assigned_interface_id: string | null;
  assigned_interface_name: string | null;
  speed_bps: number;
  retries: number;
}

export interface ActivePathContribution {
  interface_id: string;
  interface_name: string;
  kind: InterfaceKind;
  speed_bps: number;
  bytes_contributed: number;
  via_tunnel: boolean;
}

export interface DownloadItem {
  id: string;
  url: string;
  file_name: string;
  save_dir: string;
  target_path: string;
  Category?: string;
  total_bytes: number | null;
  downloaded_bytes: number;
  status: DownloadStatus;
  priority: DownloadPriority;
  supports_range: boolean;
  speed_bps: number;
  avg_speed_bps: number;
  eta_seconds: number | null;
  speed_limit_bps: number | null;
  scheduled_at_ms: number | null;
  expected_checksum: string | null;
  checksum_algorithm: "SHA256" | "MD5" | null;
  computed_checksum: string | null;
  checksum_verified: boolean | null;
  error_message: string | null;
  retry_count: number;
  max_retries: number;
  created_at_ms: number;
  completed_at_ms: number | null;
  elapsed_ms: number;
  used_bonding_tunnel: boolean;
  segments: DownloadSegmentInfo[];
  active_paths: ActivePathContribution[];
}

export interface AddDownloadRequest {
  url: string;
  file_name?: string;
  save_dir?: string;
  priority: DownloadPriority;
  max_segments?: number;
  speed_limit_bps?: number | null;
  scheduled_at_ms?: number | null;
  expected_checksum?: string | null;
  checksum_algorithm?: "SHA256" | "MD5" | null;
  start_immediately: boolean;
}

export interface UrlProbeResult {
  valid: boolean;
  normalized_url: string;
  suggested_file_name: string;
  total_bytes: number | null;
  supports_range: boolean;
  content_type: string | null;
  warning: string | null;
}

export interface ConnectionHistoryEvent {
  id: number;
  timestamp_ms: number;
  interface_id: string;
  interface_name: string;
  kind: InterfaceKind;
  event_type: string;
  details: string;
}

export interface SpeedTestProgress {
  phase: "Idle" | "Ping" | "Download" | "Upload" | "Complete" | "Error";
  progress_pct: number;
  current_bps: number;
  ping_ms: number | null;
  jitter_ms: number | null;
  packet_loss_pct: number;
  download_bps: number;
  upload_bps: number;
  target_label: string;
  message: string;
}

export interface SpeedTestRecord {
  id: number;
  timestamp_ms: number;
  mode_label: string;
  interface_name: string;
  server_label: string;
  download_bps: number;
  upload_bps: number;
  ping_ms: number;
  jitter_ms: number;
  packet_loss_pct: number;
}

export interface AppSettings {
  default_download_dir: string;
  max_concurrent_downloads: number;
  default_segments_per_download: number;
  adaptive_concurrency: boolean;
  global_speed_limit_bps: number | null;
  auto_retry_downloads: boolean;
  max_retries: number;
  bonding_mode: BondingMode;
  bonding_tunnel_enabled: boolean;
  bonding_server_endpoint: string;
  bonding_auth_token_set: boolean;
  bonding_auth_token_preview: string;
  enable_local_socks_proxy: boolean;
  local_socks_port: number;
  launch_at_startup: boolean;
  minimize_to_tray: boolean;
  run_in_background: boolean;
  notify_on_complete: boolean;
  theme: "dark" | "light";
  speedtest_endpoint: string;
}

export interface UpdateSettingsPayload {
  default_download_dir: string;
  max_concurrent_downloads: number;
  default_segments_per_download: number;
  adaptive_concurrency: boolean;
  global_speed_limit_bps: number | null;
  auto_retry_downloads: boolean;
  max_retries: number;
  bonding_mode: BondingMode;
  bonding_tunnel_enabled: boolean;
  bonding_server_endpoint: string;
  new_bonding_auth_token?: string | null;
  enable_local_socks_proxy: boolean;
  local_socks_port: number;
  launch_at_startup: boolean;
  minimize_to_tray: boolean;
  run_in_background: boolean;
  notify_on_complete: boolean;
  theme: "dark" | "light";
  speedtest_endpoint: string;
}

export interface LogEntry {
  id: number;
  timestamp_ms: number;
  level: "INFO" | "WARN" | "ERROR" | "SUCCESS";
  category: "NETWORK" | "BONDING" | "DOWNLOAD" | "SPEEDTEST" | "SYSTEM";
  message: string;
}

