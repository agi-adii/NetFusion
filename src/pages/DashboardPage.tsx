import React from "react";
import { motion } from "framer-motion";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
} from "recharts";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  Layers,
  Network,
  Download,
  HardDriveDownload,
  Activity,
  ShieldCheck,
  Pause,
  Play,
  FolderOpen,
  ExternalLink,
  ChevronRight,
  Clock,
  Power,
} from "lucide-react";
import clsx from "clsx";
import {
  bondingModeDisplayName,
  formatBitsPerSec,
  formatBytes,
  formatByteSpeed,
  formatDurationMs,
  formatEta,
  getDownloadDurationMs,
} from "../lib/format";
import type {
  BandwidthSample,
  BondingMode,
  DownloadItem,
  NetworkTelemetrySnapshot,
} from "../lib/types";
import {
  FileTypeIcon,
  InterfaceIcon,
  StatusPill,
  interfaceKindColors,
  interfaceKindLabel,
} from "../components/InterfaceBadge";
import type { NavTab } from "../components/Sidebar";

interface DashboardPageProps {
  telemetry: NetworkTelemetrySnapshot;
  history: BandwidthSample[];
  downloads: DownloadItem[];
  onToggleInterface: (id: string, enabled: boolean) => void;
  onSetBondingActive: (active: boolean) => void;
  onSetAutoBonding: (enabled: boolean) => void;
  onSetAdaptiveLoadBalancing: (enabled: boolean) => void;
  onSelectBondingMode: (mode: BondingMode) => void;
  onPauseDownload: (id: string) => void;
  onResumeDownload: (id: string) => void;
  onOpenFile: (id: string) => void;
  onOpenFolder: (id: string) => void;
  onNavigate: (tab: NavTab) => void;
}

export const DashboardPage: React.FC<DashboardPageProps> = ({
  telemetry,
  history,
  downloads,
  onToggleInterface,
  onSetBondingActive,
  onSetAutoBonding,
  onSetAdaptiveLoadBalancing,
  onSelectBondingMode,
  onPauseDownload,
  onResumeDownload,
  onOpenFile,
  onOpenFolder,
  onNavigate,
}) => {
  const dlSpeed = formatBitsPerSec(telemetry.total_download_bps);
  const ulSpeed = formatBitsPerSec(telemetry.total_upload_bps);
  const aggCap = formatBitsPerSec(telemetry.total_link_capacity_bps);
  const bondingActive = telemetry.bonding.bonding_active !== false;
  const autoBonding = telemetry.bonding.auto_bonding !== false;
  const adaptiveLb = telemetry.bonding.adaptive_load_balancing !== false;
  const avgLossPct = telemetry.average_packet_loss_pct ?? 0;

  const activeDownloads = downloads.filter((d) =>
    ["Downloading", "Probing", "Queued", "Verifying", "Merging"].includes(d.status)
  );
  const recentHistory = downloads
    .filter((d) => d.status === "Completed" || d.status === "Failed")
    .slice(0, 5);

  const chartData = history.map((s) => ({
    time: s.time_label,
    downloadMbps: Number((s.total_download_bps / 1_000_000).toFixed(2)),
    uploadMbps: Number((s.total_upload_bps / 1_000_000).toFixed(2)),
  }));

  return (
    <div className="space-y-5 pb-6">
      {/* Master Bonding Controls Bar */}
      <div className="glass-card rounded-2xl p-4 flex flex-wrap items-center justify-between gap-3 border border-electric-400/25">
        <div className="flex flex-wrap items-center gap-2.5">
          {bondingActive ? (
            <button
              onClick={() => onSetBondingActive(false)}
              className="inline-flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold bg-rose-500/15 text-rose-500 dark:text-rose-300 border border-rose-400/35 hover:bg-rose-500/25 transition-all"
            >
              <Power className="w-3.5 h-3.5" />
              Stop Bonding
            </button>
          ) : (
            <button
              onClick={() => onSetBondingActive(true)}
              className="inline-flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold text-white bg-gradient-to-r from-electric-500 via-indigo-500 to-lavender-500 shadow-glow-blue hover:brightness-110 transition-all"
            >
              <Power className="w-3.5 h-3.5" />
              Start Bonding
            </button>
          )}

          <span
            className={clsx(
              "px-2.5 py-1 rounded-xl text-[11px] font-semibold border",
              bondingActive
                ? "bg-emerald-500/15 text-emerald-500 dark:text-emerald-300 border-emerald-400/30"
                : "bg-slate-500/15 text-slate-400 border-slate-400/30"
            )}
          >
            {bondingActive ? "Bonding Engine Active" : "Bonding Stopped"}
          </span>

          <label className="inline-flex items-center gap-2 px-3 py-1.5 rounded-xl bg-slate-100/70 dark:bg-white/[0.04] border border-slate-200/70 dark:border-white/[0.08] cursor-pointer text-xs">
            <input
              type="checkbox"
              checked={autoBonding}
              onChange={(e) => onSetAutoBonding(e.target.checked)}
              className="accent-electric-500 rounded"
            />
            <span className="font-medium text-slate-700 dark:text-slate-200">Auto Bonding</span>
          </label>

          <label className="inline-flex items-center gap-2 px-3 py-1.5 rounded-xl bg-slate-100/70 dark:bg-white/[0.04] border border-slate-200/70 dark:border-white/[0.08] cursor-pointer text-xs">
            <input
              type="checkbox"
              checked={adaptiveLb}
              onChange={(e) => onSetAdaptiveLoadBalancing(e.target.checked)}
              className="accent-lavender-500 rounded"
            />
            <span className="font-medium text-slate-700 dark:text-slate-200">
              Adaptive Load Balancing
            </span>
          </label>
        </div>

        <div className="flex flex-wrap items-center gap-2.5">
          <div className="flex items-center gap-1.5 text-xs">
            <span className="text-slate-500 dark:text-slate-400 font-medium">
              Traffic Distribution:
            </span>
            <select
              value={telemetry.bonding.mode}
              onChange={(e) => onSelectBondingMode(e.target.value as BondingMode)}
              className="px-2.5 py-1.5 rounded-xl text-xs font-semibold bg-slate-100 dark:bg-navy-900 border border-slate-300/70 dark:border-white/10 text-slate-800 dark:text-white"
            >
              <option value="Smart">Adaptive Bonding</option>
              <option value="MaxSpeed">Speed Priority</option>
              <option value="Balanced">Stability Priority</option>
              <option value="Failover">Failover Mode</option>
              <option value="Custom">Download Acceleration</option>
            </select>
          </div>

          <button
            onClick={() => onNavigate("bonding")}
            className={clsx(
              "px-3 py-1.5 rounded-xl text-[11px] font-semibold border transition-colors",
              telemetry.bonding.tunnel_status === "Connected"
                ? "bg-emerald-500/15 text-emerald-400 border-emerald-400/30"
                : "bg-electric-500/10 text-electric-500 dark:text-electric-300 border-electric-400/25"
            )}
          >
            Server:{" "}
            {telemetry.bonding.tunnel_status === "Connected"
              ? `Connected (${telemetry.bonding.active_subflows} Subflows)`
              : "Direct Multi-Path Mode"}
          </button>
        </div>
      </div>

      {/* Top KPI Cards Grid (All Required Network Overview Metrics) */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3.5">
        <KpiCard
          icon={<ArrowDownToLine className="w-4 h-4 text-electric-400" />}
          label="Total Download Speed"
          value={dlSpeed.value}
          unit={dlSpeed.unit}
          subtext={formatByteSpeed(telemetry.total_download_bps)}
          accent="from-electric-500/20 to-indigo-500/10 border-electric-400/30"
        />
        <KpiCard
          icon={<ArrowUpFromLine className="w-4 h-4 text-lavender-400" />}
          label="Total Upload Speed"
          value={ulSpeed.value}
          unit={ulSpeed.unit}
          subtext={formatByteSpeed(telemetry.total_upload_bps)}
          accent="from-lavender-500/20 to-purple-500/10 border-lavender-400/30"
        />
        <KpiCard
          icon={<Layers className="w-4 h-4 text-emerald-400" />}
          label="Combined Bandwidth"
          value={aggCap.value}
          unit={aggCap.unit}
          subtext={`Mode: ${bondingModeDisplayName(telemetry.bonding.mode)}`}
          accent="from-emerald-500/15 to-teal-500/10 border-emerald-400/25"
        />
        <KpiCard
          icon={<Network className="w-4 h-4 text-sky-400" />}
          label="Active Connections"
          value={String(telemetry.active_connections_count)}
          unit={`/ ${telemetry.interfaces.length} NICs`}
          subtext={
            telemetry.bonding.tunnel_status === "Connected"
              ? `${telemetry.bonding.active_subflows} Tunnel Subflows`
              : "Multi-Path Bonding Ready"
          }
          accent="from-sky-500/15 to-blue-500/10 border-sky-400/25"
        />
        <KpiCard
          icon={<Download className="w-4 h-4 text-indigo-400" />}
          label="Active Downloads"
          value={String(activeDownloads.length)}
          unit="Tasks"
          subtext={`${downloads.filter((d) => d.status === "Completed").length} Completed`}
          accent="from-indigo-500/15 to-lavender-500/10 border-indigo-400/25"
        />
        <KpiCard
          icon={<HardDriveDownload className="w-4 h-4 text-cyan-400" />}
          label="Downloaded Data Today"
          value={formatBytes(telemetry.downloaded_today_bytes)}
          unit=""
          subtext={`Uploaded: ${formatBytes(telemetry.uploaded_today_bytes)}`}
          accent="from-cyan-500/15 to-electric-500/10 border-cyan-400/25"
        />
        <KpiCard
          icon={<Activity className="w-4 h-4 text-amber-400" />}
          label="Latency & Packet Loss"
          value={
            telemetry.average_latency_ms !== null
              ? `${Math.round(telemetry.average_latency_ms)}`
              : "—"
          }
          unit={telemetry.average_latency_ms !== null ? "ms" : ""}
          subtext={`Packet Loss: ${avgLossPct.toFixed(1)}%`}
          accent="from-amber-500/15 to-orange-500/10 border-amber-400/25"
        />
        <KpiCard
          icon={<ShieldCheck className="w-4 h-4 text-emerald-400" />}
          label="Network Health"
          value={`${Math.round(telemetry.overall_stability_pct)}`}
          unit="%"
          subtext={
            telemetry.overall_stability_pct >= 90
              ? "Optimal multi-path health"
              : telemetry.overall_stability_pct >= 70
              ? "Moderate link variance"
              : "Check unstable links"
          }
          accent="from-emerald-500/15 to-cyan-500/10 border-emerald-400/25"
        />
      </div>

      {/* Middle Hero Section: Large Live Aggregate Speed Gauge + Real-time Bandwidth Graph */}
      <div className="grid grid-cols-1 xl:grid-cols-12 gap-4">
        {/* Hero Aggregate Gauge Card */}
        <div className="xl:col-span-4 glass-card rounded-2xl p-5 flex flex-col justify-between relative overflow-hidden">
          <div className="absolute -right-12 -top-12 w-44 h-44 rounded-full bg-gradient-to-br from-electric-500/20 to-lavender-500/20 blur-2xl pointer-events-none" />
          <div>
            <div className="flex items-center justify-between">
              <span className="text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">
                Aggregate Throughput
              </span>
              <span className="px-2.5 py-0.5 rounded-full text-[11px] font-semibold bg-electric-500/15 text-electric-500 dark:text-electric-300 border border-electric-400/30">
                LIVE 1 Hz
              </span>
            </div>

            <div className="my-6 flex flex-col items-center justify-center text-center">
              <div className="relative w-44 h-44 rounded-full flex flex-col items-center justify-center border-4 border-electric-400/25 bg-gradient-to-b from-electric-500/10 via-transparent to-lavender-500/10 shadow-glow-blue">
                <span className="text-[11px] uppercase tracking-widest text-slate-500 dark:text-slate-400">
                  Combined Down
                </span>
                <motion.span
                  key={dlSpeed.value}
                  initial={{ opacity: 0.75, scale: 0.98 }}
                  animate={{ opacity: 1, scale: 1 }}
                  className="text-4xl font-extrabold tabular-nums tracking-tight bg-gradient-to-r from-electric-400 via-sky-300 to-lavender-400 bg-clip-text text-transparent mt-1"
                >
                  {dlSpeed.value}
                </motion.span>
                <span className="text-xs font-semibold text-electric-500 dark:text-electric-300">
                  {dlSpeed.unit} ({formatByteSpeed(telemetry.total_download_bps)})
                </span>
              </div>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-2.5 pt-3 border-t border-slate-200/70 dark:border-white/[0.07]">
            <div className="p-2.5 rounded-xl bg-slate-100/70 dark:bg-white/[0.03]">
              <div className="text-[11px] text-slate-500 dark:text-slate-400">Combined Upload</div>
              <div className="text-sm font-bold tabular-nums text-slate-900 dark:text-white mt-0.5">
                {ulSpeed.full}
              </div>
            </div>
            <div className="p-2.5 rounded-xl bg-slate-100/70 dark:bg-white/[0.03]">
              <div className="text-[11px] text-slate-500 dark:text-slate-400">Bonding Engine</div>
              <div className="text-sm font-bold text-lavender-500 dark:text-lavender-300 mt-0.5 truncate">
                {bondingModeDisplayName(telemetry.bonding.mode)}
              </div>
            </div>
          </div>
        </div>

        {/* Live Aggregate Bandwidth Graph */}
        <div className="xl:col-span-8 glass-card rounded-2xl p-5 flex flex-col">
          <div className="flex flex-wrap items-center justify-between gap-2 mb-4">
            <div>
              <h2 className="text-sm font-bold text-slate-900 dark:text-white">
                Live Aggregate Internet Speed
              </h2>
              <p className="text-xs text-slate-500 dark:text-slate-400">
                Real-time combined traffic across all active Windows network interfaces (Mbps)
              </p>
            </div>
            <div className="flex items-center gap-4 text-xs">
              <span className="flex items-center gap-1.5 text-electric-500 dark:text-electric-400 font-medium">
                <span className="w-2.5 h-2.5 rounded-full bg-electric-400" />
                Download (Mbps)
              </span>
              <span className="flex items-center gap-1.5 text-lavender-500 dark:text-lavender-400 font-medium">
                <span className="w-2.5 h-2.5 rounded-full bg-lavender-400" />
                Upload (Mbps)
              </span>
            </div>
          </div>

          <div className="flex-1 min-h-[235px]">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={chartData} margin={{ top: 8, right: 8, left: -16, bottom: 0 }}>
                <defs>
                  <linearGradient id="dlGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="#38bdf8" stopOpacity={0.45} />
                    <stop offset="95%" stopColor="#38bdf8" stopOpacity={0.0} />
                  </linearGradient>
                  <linearGradient id="ulGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="#a78bfa" stopOpacity={0.4} />
                    <stop offset="95%" stopColor="#a78bfa" stopOpacity={0.0} />
                  </linearGradient>
                </defs>
                <CartesianGrid strokeDasharray="3 3" stroke="rgba(148, 163, 184, 0.12)" />
                <XAxis
                  dataKey="time"
                  tick={{ fontSize: 11, fill: "#64748b" }}
                  axisLine={false}
                  tickLine={false}
                />
                <YAxis
                  tick={{ fontSize: 11, fill: "#64748b" }}
                  axisLine={false}
                  tickLine={false}
                />
                <Tooltip
                  contentStyle={{
                    backgroundColor: "rgba(9, 14, 33, 0.92)",
                    borderColor: "rgba(167, 139, 250, 0.3)",
                    borderRadius: "12px",
                    fontSize: "12px",
                    color: "#f8fafc",
                  }}
                />
                <Area
                  type="monotone"
                  dataKey="downloadMbps"
                  name="Download (Mbps)"
                  stroke="#38bdf8"
                  strokeWidth={2.4}
                  fill="url(#dlGrad)"
                  isAnimationActive={false}
                />
                <Area
                  type="monotone"
                  dataKey="uploadMbps"
                  name="Upload (Mbps)"
                  stroke="#a78bfa"
                  strokeWidth={2}
                  fill="url(#ulGrad)"
                  isAnimationActive={false}
                />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        </div>
      </div>

      {/* Bottom Section: Active Network Interface Cards + Active Downloads & Recent History */}
      <div className="grid grid-cols-1 xl:grid-cols-12 gap-4">
        {/* Active Network Interface Cards */}
        <div className="xl:col-span-5 glass-card rounded-2xl p-5 space-y-3.5">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-sm font-bold text-slate-900 dark:text-white">
                Network Interfaces
              </h2>
              <p className="text-xs text-slate-500 dark:text-slate-400">
                Enable or disable adapters participating in bonding
              </p>
            </div>
            <button
              onClick={() => onNavigate("bonding")}
              className="inline-flex items-center gap-1 text-xs font-semibold text-electric-500 dark:text-electric-400 hover:underline"
            >
              Configure
              <ChevronRight className="w-3.5 h-3.5" />
            </button>
          </div>

            <div className="space-y-2.5 max-h-[380px] overflow-y-auto pr-1">
            {telemetry.interfaces.length === 0 ? (
              <div className="p-6 text-center text-xs text-slate-500 dark:text-slate-400">
                Detecting Windows network adapters...
              </div>
            ) : (
              telemetry.interfaces.map((nic) => {
                const colors = interfaceKindColors(nic.kind);
                const dl = formatBitsPerSec(nic.download_bps);
                const peakDl = formatBitsPerSec(nic.peak_download_bps || 0);
                return (
                  <div
                    key={nic.id}
                    className={clsx(
                      "p-3.5 rounded-xl border transition-all",
                      nic.enabled_for_bonding && nic.status !== "Disconnected"
                        ? "bg-slate-100/70 dark:bg-white/[0.04] border-electric-400/25"
                        : "bg-slate-100/30 dark:bg-white/[0.015] border-slate-200/60 dark:border-white/[0.06] opacity-75"
                    )}
                  >
                    <div className="flex items-center justify-between gap-2">
                      <div className="flex items-center gap-2.5 min-w-0">
                        <span className={clsx("p-2 rounded-xl border", colors.badge)}>
                          <InterfaceIcon kind={nic.kind} className="w-4 h-4" />
                        </span>
                        <div className="min-w-0">
                          <div className="flex flex-wrap items-center gap-1.5">
                            {nic.slot_label && (
                              <span className="px-1.5 py-0.5 rounded-md text-[10px] font-bold bg-electric-500/15 text-electric-500 dark:text-electric-300 border border-electric-400/25">
                                {nic.slot_label}
                              </span>
                            )}
                            <span className="text-xs font-bold text-slate-900 dark:text-white truncate">
                              {nic.name}
                            </span>
                            <StatusPill status={nic.status} />
                          </div>
                          <div className="text-[11px] text-slate-500 dark:text-slate-400 truncate mt-0.5">
                            {interfaceKindLabel(nic.kind)}
                            {nic.wifi_ssid ? ` · SSID: ${nic.wifi_ssid}` : ""}
                            {nic.wifi_signal_pct != null ? ` (${nic.wifi_signal_pct}%)` : ""} ·{" "}
                            {nic.ipv4 || "No IPv4"} · Ping:{" "}
                            {nic.latency_ms !== null ? `${Math.round(nic.latency_ms)} ms` : "—"}
                          </div>
                        </div>
                      </div>

                      <label className="relative inline-flex items-center cursor-pointer shrink-0">
                        <input
                          type="checkbox"
                          checked={nic.enabled_for_bonding}
                          onChange={(e) => onToggleInterface(nic.id, e.target.checked)}
                          className="sr-only peer"
                        />
                        <div className="w-9 h-5 bg-slate-300 dark:bg-slate-700 rounded-full peer peer-checked:bg-gradient-to-r peer-checked:from-electric-500 peer-checked:to-lavender-500 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:after:translate-x-4" />
                      </label>
                    </div>

                    <div className="mt-2.5 pt-2 border-t border-slate-200/60 dark:border-white/[0.06] flex flex-wrap items-center justify-between gap-2 text-[11px] tabular-nums">
                      <span className="text-electric-600 dark:text-electric-400 font-semibold">
                        ↓ {dl.full}
                      </span>
                      <span className="text-lavender-600 dark:text-lavender-300">
                        ↑ {formatBitsPerSec(nic.upload_bps).full}
                      </span>
                      <span className="text-emerald-600 dark:text-emerald-400">
                        Max: {peakDl.full}
                      </span>
                      <span className="text-slate-500 dark:text-slate-400">
                        Share: <strong>{Math.round(nic.allocated_weight_pct)}%</strong>
                      </span>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </div>

        {/* Active Download Tasks + Recent History */}
        <div className="xl:col-span-7 space-y-4">
          {/* Active Download Tasks */}
          <div className="glass-card rounded-2xl p-5 space-y-3">
            <div className="flex items-center justify-between">
              <div>
                <h2 className="text-sm font-bold text-slate-900 dark:text-white">
                  Active Download Tasks ({activeDownloads.length})
                </h2>
                <p className="text-xs text-slate-500 dark:text-slate-400">
                  Parallel multi-segment transfers across active network paths
                </p>
              </div>
              <button
                onClick={() => onNavigate("downloads")}
                className="inline-flex items-center gap-1 text-xs font-semibold text-electric-500 dark:text-electric-400 hover:underline"
              >
                View All
                <ChevronRight className="w-3.5 h-3.5" />
              </button>
            </div>

            {activeDownloads.length === 0 ? (
              <div className="py-8 text-center text-xs text-slate-500 dark:text-slate-400 border border-dashed border-slate-300/60 dark:border-white/10 rounded-xl">
                No active downloads right now. Click <strong>New Download</strong> to accelerate a file across your networks.
              </div>
            ) : (
              <div className="space-y-2.5">
                {activeDownloads.slice(0, 3).map((item) => {
                  const pct =
                    item.total_bytes && item.total_bytes > 0
                      ? Math.min(100, (item.downloaded_bytes / item.total_bytes) * 100)
                      : 0;
                  return (
                    <div
                      key={item.id}
                      className="p-3.5 rounded-xl bg-slate-100/70 dark:bg-white/[0.035] border border-slate-200/70 dark:border-white/[0.08]"
                    >
                      <div className="flex items-center justify-between gap-3">
                        <div className="flex items-center gap-3 min-w-0">
                          <FileTypeIcon fileName={item.file_name} className="w-4 h-4" />
                          <div className="min-w-0">
                            <div className="text-xs font-bold text-slate-900 dark:text-white truncate">
                              {item.file_name}
                            </div>
                            <div className="text-[11px] text-slate-500 dark:text-slate-400 tabular-nums">
                              {formatBytes(item.downloaded_bytes)} /{" "}
                              {formatBytes(item.total_bytes)} ·{" "}
                              <span className="text-electric-500 dark:text-electric-400 font-semibold">
                                {formatByteSpeed(item.speed_bps)}
                              </span>{" "}
                              · Time: <strong>{formatDurationMs(getDownloadDurationMs(item))}</strong>{" "}
                              · ETA {formatEta(item.eta_seconds)}
                            </div>
                          </div>
                        </div>

                        {item.status === "Downloading" ||
                        item.status === "Probing" ||
                        item.status === "Queued" ? (
                          <button
                            onClick={() => onPauseDownload(item.id)}
                            className="p-2 rounded-lg glass-card text-amber-400 hover:bg-amber-500/15"
                            title="Pause Download"
                          >
                            <Pause className="w-3.5 h-3.5" />
                          </button>
                        ) : (
                          <button
                            onClick={() => onResumeDownload(item.id)}
                            className="p-2 rounded-lg glass-card text-emerald-400 hover:bg-emerald-500/15"
                            title="Resume from Paused Offset"
                          >
                            <Play className="w-3.5 h-3.5" />
                          </button>
                        )}
                      </div>

                      <div className="mt-2.5 h-2 rounded-full bg-slate-200 dark:bg-navy-950 overflow-hidden">
                        <div
                          className="h-full rounded-full bg-gradient-to-r from-electric-400 via-indigo-400 to-lavender-400 transition-all duration-300"
                          style={{ width: `${pct.toFixed(1)}%` }}
                        />
                      </div>

                      {item.active_paths.length > 0 && (
                        <div className="mt-2 flex flex-wrap items-center gap-1.5">
                          {item.active_paths.map((p) => (
                            <span
                              key={p.interface_id}
                              className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-medium bg-electric-500/10 text-electric-600 dark:text-electric-300 border border-electric-400/20 tabular-nums"
                            >
                              <InterfaceIcon kind={p.kind} className="w-3 h-3" />
                              {p.interface_name}: {formatByteSpeed(p.speed_bps)}
                            </span>
                          ))}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          {/* Recent Download History */}
          <div className="glass-card rounded-2xl p-5 space-y-2.5">
            <h2 className="text-sm font-bold text-slate-900 dark:text-white">
              Recent Download History
            </h2>
            {recentHistory.length === 0 ? (
              <p className="text-xs text-slate-500 dark:text-slate-400 py-3">
                Completed and failed downloads will appear here.
              </p>
            ) : (
              <div className="divide-y divide-slate-200/60 dark:divide-white/[0.06]">
                {recentHistory.map((d) => (
                  <div key={d.id} className="py-2 flex items-center justify-between gap-3">
                    <div className="min-w-0">
                      <div className="text-xs font-semibold text-slate-800 dark:text-slate-200 truncate">
                        {d.file_name}
                      </div>
                      <div className="text-[11px] text-slate-500 dark:text-slate-400 tabular-nums flex flex-wrap items-center gap-1.5">
                        <span>{formatBytes(d.total_bytes || d.downloaded_bytes)}</span>
                        <span>·</span>
                        <span
                          className={
                            d.status === "Completed"
                              ? "text-emerald-500 dark:text-emerald-400 font-semibold"
                              : "text-rose-500 dark:text-rose-400 font-semibold"
                          }
                        >
                          {d.status}
                        </span>
                        <span>·</span>
                        <span className="inline-flex items-center gap-1 text-electric-600 dark:text-electric-300 font-semibold">
                          <Clock className="w-3 h-3" />
                          Total Time: {formatDurationMs(getDownloadDurationMs(d))}
                        </span>
                        {d.avg_speed_bps > 0 && (
                          <>
                            <span>·</span>
                            <span>Avg {formatByteSpeed(d.avg_speed_bps)}</span>
                          </>
                        )}
                      </div>
                    </div>
                    {d.status === "Completed" && (
                      <div className="flex items-center gap-1.5 shrink-0">
                        <button
                          onClick={() => onOpenFile(d.id)}
                          title="Open File"
                          className="p-1.5 rounded-lg hover:bg-slate-200/70 dark:hover:bg-white/10 text-slate-500 dark:text-slate-300"
                        >
                          <ExternalLink className="w-3.5 h-3.5" />
                        </button>
                        <button
                          onClick={() => onOpenFolder(d.id)}
                          title="Open Folder"
                          className="p-1.5 rounded-lg hover:bg-slate-200/70 dark:hover:bg-white/10 text-slate-500 dark:text-slate-300"
                        >
                          <FolderOpen className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

const KpiCard: React.FC<{
  icon: React.ReactNode;
  label: string;
  value: string;
  unit: string;
  subtext: string;
  accent: string;
}> = ({ icon, label, value, unit, subtext, accent }) => (
  <div
    className={clsx(
      "glass-card rounded-2xl p-4 bg-gradient-to-br border transition-transform hover:-translate-y-0.5",
      accent
    )}
  >
    <div className="flex items-center justify-between">
      <span className="text-xs font-medium text-slate-600 dark:text-slate-300 truncate">
        {label}
      </span>
      <div className="p-1.5 rounded-lg bg-white/60 dark:bg-navy-950/60">{icon}</div>
    </div>
    <div className="mt-2.5 flex items-baseline gap-1.5">
      <span className="text-2xl font-extrabold tabular-nums text-slate-900 dark:text-white">
        {value}
      </span>
      {unit && (
        <span className="text-xs font-semibold text-slate-500 dark:text-slate-400">{unit}</span>
      )}
    </div>
    <div className="mt-1 text-[11px] text-slate-500 dark:text-slate-400 truncate">{subtext}</div>
  </div>
);

