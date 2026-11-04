import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
  AddDownloadRequest,
  AppSettings,
  BondingMode,
  ConnectionHistoryEvent,
  DownloadItem,
  DownloadPriority,
  LogEntry,
  NetworkTelemetrySnapshot,
  SpeedTestProgress,
  SpeedTestRecord,
  UpdateSettingsPayload,
  UrlProbeResult,
  WifiNetworkScanItem,
} from "./types";

export const isTauriRuntime = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const api = {
  getInitialTelemetry: () => invoke<NetworkTelemetrySnapshot>("get_network_telemetry"),
  setInterfaceEnabled: (interfaceId: string, enabled: boolean) =>
    invoke<void>("set_interface_enabled", { interfaceId, enabled }),
  setInterfaceCustomWeight: (interfaceId: string, weight: number) =>
    invoke<void>("set_interface_custom_weight", { interfaceId, weight }),
  setPrimaryFailoverInterface: (interfaceId: string) =>
    invoke<void>("set_primary_failover_interface", { interfaceId }),
  setBondingMode: (mode: BondingMode) => invoke<void>("set_bonding_mode", { mode }),
  connectBondingServer: (endpoint: string, authToken?: string | null) =>
    invoke<void>("connect_bonding_server", { endpoint, authToken }),
  disconnectBondingServer: () => invoke<void>("disconnect_bonding_server"),
  setBondingActive: (active: boolean) => invoke<void>("set_bonding_active", { active }),
  setAutoBonding: (enabled: boolean) => invoke<void>("set_auto_bonding", { enabled }),
  setAdaptiveLoadBalancing: (enabled: boolean) =>
    invoke<void>("set_adaptive_load_balancing", { enabled }),
  setSystemProxyRouting: (enabled: boolean) =>
    invoke<void>("set_system_proxy_routing", { enabled }),
  scanWifiNetworks: (interfaceName: string) =>
    invoke<WifiNetworkScanItem[]>("scan_wifi_networks", { interfaceName }),
  connectWifiNetwork: (interfaceName: string, ssid: string, password?: string | null) =>
    invoke<string>("connect_wifi_network", { interfaceName, ssid, password }),
  disconnectWifiNetwork: (interfaceName: string) =>
    invoke<void>("disconnect_wifi_network", { interfaceName }),
  getConnectionHistory: () => invoke<ConnectionHistoryEvent[]>("get_connection_history"),

  // Downloads
  listDownloads: () => invoke<DownloadItem[]>("list_downloads"),
  probeDownloadUrl: (url: string) => invoke<UrlProbeResult>("probe_download_url", { url }),
  addDownload: (request: AddDownloadRequest) => invoke<DownloadItem>("add_download", { request }),
  pauseDownload: (downloadId: string) => invoke<void>("pause_download", { downloadId }),
  resumeDownload: (downloadId: string) => invoke<void>("resume_download", { downloadId }),
  cancelDownload: (downloadId: string, deleteFiles: boolean) =>
    invoke<void>("cancel_download", { downloadId, deleteFiles }),
  retryDownload: (downloadId: string) => invoke<void>("retry_download", { downloadId }),
  updateDownloadPriority: (downloadId: string, priority: DownloadPriority) =>
    invoke<void>("update_download_priority", { downloadId, priority }),
  updateDownloadSpeedLimit: (downloadId: string, speedLimitBps: number | null) =>
    invoke<void>("update_download_speed_limit", { downloadId, speedLimitBps }),
  pauseAllDownloads: () => invoke<void>("pause_all_downloads"),
  resumeAllDownloads: () => invoke<void>("resume_all_downloads"),
  clearCompletedDownloads: () => invoke<void>("clear_completed_downloads"),
  openDownloadedFile: (downloadId: string) => invoke<void>("open_downloaded_file", { downloadId }),
  openDownloadFolder: (downloadId: string) => invoke<void>("open_download_folder", { downloadId }),

  // Speed Test
  runSpeedTest: (interfaceId: string | null) =>
    invoke<SpeedTestRecord>("run_speed_test", { interfaceId }),
  cancelSpeedTest: () => invoke<void>("cancel_speed_test"),
  getSpeedTestHistory: () => invoke<SpeedTestRecord[]>("get_speed_test_history"),

  // Settings & Logs
  getSettings: () => invoke<AppSettings>("get_settings"),
  updateSettings: (payload: UpdateSettingsPayload) =>
    invoke<AppSettings>("update_settings", { payload }),
  getLogs: () => invoke<LogEntry[]>("get_logs"),
  clearLogs: () => invoke<void>("clear_logs"),
};

export async function subscribeEvent<T>(
  eventName: string,
  handler: (payload: T) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  return listen<T>(eventName, (event) => handler(event.payload));
}

