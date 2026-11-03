import React, { useEffect, useState, useCallback } from "react";
import { Sidebar, NavTab } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { AddDownloadModal } from "./components/AddDownloadModal";
import { DashboardPage } from "./pages/DashboardPage";
import { DownloadsPage } from "./pages/DownloadsPage";
import { BondingPage } from "./pages/BondingPage";
import { MonitorPage } from "./pages/MonitorPage";
import { SpeedTestPage } from "./pages/SpeedTestPage";
import { SettingsPage } from "./pages/SettingsPage";
import { LogsPage } from "./pages/LogsPage";
import { api, isTauriRuntime, subscribeEvent } from "./lib/tauri";
import { formatTimeShort } from "./lib/format";
import type {
  AppSettings,
  BandwidthSample,
  BondingMode,
  ConnectionHistoryEvent,
  DownloadItem,
  DownloadPriority,
  LogEntry,
  NetworkTelemetrySnapshot,
  SpeedTestProgress,
  SpeedTestRecord,
  UpdateSettingsPayload,
} from "./lib/types";

const INITIAL_TELEMETRY: NetworkTelemetrySnapshot = {
  timestamp_ms: Date.now(),
  total_download_bps: 0,
  total_upload_bps: 0,
  total_link_capacity_bps: 0,
  active_connections_count: 0,
  average_latency_ms: null,
  overall_stability_pct: 100,
  downloaded_today_bytes: 0,
  uploaded_today_bytes: 0,
  interfaces: [],
  bonding: {
    mode: "Smart",
    tunnel_enabled: false,
    tunnel_status: "Disconnected",
    server_endpoint: "127.0.0.1:9443",
    active_subflows: 0,
    local_socks_port: 10808,
    tunnel_rtt_ms: null,
    reassembly_queue_depth: 0,
    total_bonded_rx_bytes: 0,
    total_bonded_tx_bytes: 0,
    status_message: "Direct Multi-Path Mode active",
  },
};

const INITIAL_SETTINGS: AppSettings = {
  default_download_dir: "",
  max_concurrent_downloads: 4,
  default_segments_per_download: 8,
  adaptive_concurrency: true,
  global_speed_limit_bps: null,
  auto_retry_downloads: true,
  max_retries: 5,
  bonding_mode: "Smart",
  bonding_tunnel_enabled: false,
  bonding_server_endpoint: "127.0.0.1:9443",
  bonding_auth_token_set: true,
  bonding_auth_token_preview: "Configured (SHA-256 Protected)",
  enable_local_socks_proxy: true,
  local_socks_port: 10808,
  launch_at_startup: false,
  minimize_to_tray: true,
  run_in_background: true,
  notify_on_complete: true,
  theme: "dark",
  speedtest_endpoint: "https://speed.cloudflare.com",
};

const INITIAL_SPEEDTEST_PROGRESS: SpeedTestProgress = {
  phase: "Idle",
  progress_pct: 0,
  current_bps: 0,
  ping_ms: null,
  jitter_ms: null,
  packet_loss_pct: 0,
  download_bps: 0,
  upload_bps: 0,
  target_label: "Aggregate Bonding",
  message: "Ready",
};

const MAX_HISTORY_POINTS = 60;

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<NavTab>("dashboard");
  const [telemetry, setTelemetry] = useState<NetworkTelemetrySnapshot>(INITIAL_TELEMETRY);
  const [bandwidthHistory, setBandwidthHistory] = useState<BandwidthSample[]>([]);
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [connectionEvents, setConnectionEvents] = useState<ConnectionHistoryEvent[]>([]);
  const [speedTestProgress, setSpeedTestProgress] = useState<SpeedTestProgress>(
    INITIAL_SPEEDTEST_PROGRESS
  );
  const [speedTestRecords, setSpeedTestRecords] = useState<SpeedTestRecord[]>([]);
  const [settings, setSettings] = useState<AppSettings>(INITIAL_SETTINGS);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [addModalOpen, setAddModalOpen] = useState(false);
  const [theme, setTheme] = useState<"dark" | "light">("dark");

  // Apply theme class to <html>
  useEffect(() => {
    const root = document.documentElement;
    if (theme === "dark") {
      root.classList.add("dark");
    } else {
      root.classList.remove("dark");
    }
  }, [theme]);

  const appendTelemetrySample = useCallback((snap: NetworkTelemetrySnapshot) => {
    setTelemetry(snap);
    let wifiBps = 0;
    let ethBps = 0;
    let usbBps = 0;
    let otherBps = 0;
    const perIf: BandwidthSample["per_interface"] = {};

    for (const nic of snap.interfaces) {
      perIf[nic.id] = {
        download_bps: nic.download_bps,
        upload_bps: nic.upload_bps,
        latency_ms: nic.latency_ms,
      };
      if (nic.kind === "WiFi") wifiBps += nic.download_bps;
      else if (nic.kind === "Ethernet") ethBps += nic.download_bps;
      else if (nic.kind === "UsbTethering" || nic.kind === "Cellular") usbBps += nic.download_bps;
      else otherBps += nic.download_bps;
    }

    const sample: BandwidthSample = {
      timestamp_ms: snap.timestamp_ms,
      time_label: formatTimeShort(snap.timestamp_ms),
      total_download_bps: snap.total_download_bps,
      total_upload_bps: snap.total_upload_bps,
      wifi_download_bps: wifiBps,
      ethernet_download_bps: ethBps,
      usb_download_bps: usbBps,
      other_download_bps: otherBps,
      per_interface: perIf,
    };

    setBandwidthHistory((prev) => {
      const next = [...prev, sample];
      return next.length > MAX_HISTORY_POINTS
        ? next.slice(next.length - MAX_HISTORY_POINTS)
        : next;
    });
  }, []);

  const refreshAll = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const [snap, dlList, st, hist, spHist, logList] = await Promise.all([
        api.getInitialTelemetry(),
        api.listDownloads(),
        api.getSettings(),
        api.getConnectionHistory(),
        api.getSpeedTestHistory(),
        api.getLogs(),
      ]);
      appendTelemetrySample(snap);
      setDownloads(dlList);
      setSettings(st);
      setTheme(st.theme);
      setConnectionEvents(hist);
      setSpeedTestRecords(spHist);
      setLogs(logList);
    } catch (e) {
      console.error("Initial hydration error:", e);
    }
  }, [appendTelemetrySample]);

  useEffect(() => {
    refreshAll();

    const unlisteners: Array<() => void> = [];
    (async () => {
      unlisteners.push(
        await subscribeEvent<NetworkTelemetrySnapshot>("network-telemetry", (snap) => {
          appendTelemetrySample(snap);
        })
      );
      unlisteners.push(
        await subscribeEvent<DownloadItem[]>("downloads-updated", (items) => {
          setDownloads(items);
        })
      );
      unlisteners.push(
        await subscribeEvent<SpeedTestProgress>("speedtest-progress", (prog) => {
          setSpeedTestProgress(prog);
        })
      );
      unlisteners.push(
        await subscribeEvent<LogEntry>("log-added", (entry) => {
          setLogs((prev) => [entry, ...prev].slice(0, 500));
        })
      );
      unlisteners.push(
        await subscribeEvent<ConnectionHistoryEvent>("connection-event", (ev) => {
          setConnectionEvents((prev) => [ev, ...prev].slice(0, 200));
        })
      );
    })();

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, [refreshAll, appendTelemetrySample]);

  const handleToggleTheme = async () => {
    const nextTheme = theme === "dark" ? "light" : "dark";
    setTheme(nextTheme);
    if (isTauriRuntime()) {
      const updated = await api.updateSettings({
        ...settings,
        theme: nextTheme,
        new_bonding_auth_token: null,
      });
      setSettings(updated);
    }
  };

  const activeDownloadsCount = downloads.filter((d) =>
    ["Downloading", "Probing", "Queued", "Verifying", "Merging"].includes(d.status)
  ).length;

  return (
    <div className="h-screen w-screen overflow-hidden flex app-backdrop text-slate-900 dark:text-slate-100">
      <Sidebar
        activeTab={activeTab}
        onSelectTab={setActiveTab}
        activeDownloadsCount={activeDownloadsCount}
        totalDownloadBps={telemetry.total_download_bps}
        activeInterfacesCount={telemetry.active_connections_count}
        bondingMode={telemetry.bonding.mode}
        tunnelStatus={telemetry.bonding.tunnel_status}
      />

      <div className="flex-1 flex flex-col min-w-0 h-full">
        <TopBar
          activeTab={activeTab}
          activeConnectionsCount={telemetry.active_connections_count}
          bonding={telemetry.bonding}
          theme={theme}
          onToggleTheme={handleToggleTheme}
          onOpenAddDownload={() => setAddModalOpen(true)}
          onNavigate={setActiveTab}
        />

        <main className="flex-1 overflow-y-auto px-6 py-5">
          {activeTab === "dashboard" && (
            <DashboardPage
              telemetry={telemetry}
              history={bandwidthHistory}
              downloads={downloads}
              onToggleInterface={(id, en) => api.setInterfaceEnabled(id, en)}
              onPauseDownload={(id) => api.pauseDownload(id)}
              onResumeDownload={(id) => api.resumeDownload(id)}
              onOpenFile={(id) => api.openDownloadedFile(id)}
              onOpenFolder={(id) => api.openDownloadFolder(id)}
              onNavigate={setActiveTab}
            />
          )}

          {activeTab === "downloads" && (
            <DownloadsPage
              downloads={downloads}
              onOpenAddModal={() => setAddModalOpen(true)}
              onPause={(id) => api.pauseDownload(id)}
              onResume={(id) => api.resumeDownload(id)}
              onRetry={(id) => api.retryDownload(id)}
              onCancel={(id, del) => api.cancelDownload(id, del)}
              onUpdatePriority={(id, p: DownloadPriority) =>
                api.updateDownloadPriority(id, p)
              }
              onUpdateSpeedLimit={(id, lim) => api.updateDownloadSpeedLimit(id, lim)}
              onPauseAll={() => api.pauseAllDownloads()}
              onResumeAll={() => api.resumeAllDownloads()}
              onClearCompleted={() => api.clearCompletedDownloads()}
              onOpenFile={(id) => api.openDownloadedFile(id)}
              onOpenFolder={(id) => api.openDownloadFolder(id)}
            />
          )}

          {activeTab === "bonding" && (
            <BondingPage
              interfaces={telemetry.interfaces}
              bonding={telemetry.bonding}
              onSelectMode={(m: BondingMode) => api.setBondingMode(m)}
              onToggleInterface={(id, en) => api.setInterfaceEnabled(id, en)}
              onChangeCustomWeight={(id, w) => api.setInterfaceCustomWeight(id, w)}
              onSetPrimaryFailover={(id) => api.setPrimaryFailoverInterface(id)}
              onConnectServer={(ep, tok) => api.connectBondingServer(ep, tok)}
              onDisconnectServer={() => api.disconnectBondingServer()}
            />
          )}

          {activeTab === "monitor" && (
            <MonitorPage
              telemetry={telemetry}
              history={bandwidthHistory}
              connectionEvents={connectionEvents}
            />
          )}

          {activeTab === "speedtest" && (
            <SpeedTestPage
              interfaces={telemetry.interfaces}
              progress={speedTestProgress}
              records={speedTestRecords}
              onStartTest={async (ifId) => {
                await api.runSpeedTest(ifId);
                const updated = await api.getSpeedTestHistory();
                setSpeedTestRecords(updated);
              }}
              onCancelTest={() => api.cancelSpeedTest()}
            />
          )}

          {activeTab === "settings" && (
            <SettingsPage
              settings={settings}
              onSave={async (payload: UpdateSettingsPayload) => {
                const updated = await api.updateSettings(payload);
                setSettings(updated);
                setTheme(updated.theme);
              }}
            />
          )}

          {activeTab === "logs" && (
            <LogsPage
              logs={logs}
              onClear={async () => {
                await api.clearLogs();
                setLogs([]);
              }}
            />
          )}
        </main>
      </div>

      <AddDownloadModal
        isOpen={addModalOpen}
        defaultSaveDir={settings.default_download_dir}
        defaultSegments={settings.default_segments_per_download}
        onClose={() => setAddModalOpen(false)}
        onAdded={async () => {
          if (isTauriRuntime()) {
            setDownloads(await api.listDownloads());
          }
        }}
      />
    </div>
  );
};

export default App;

