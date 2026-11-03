import React from "react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
  LineChart,
  Line,
} from "recharts";
import {
  Wifi,
  Network,
  Smartphone,
  AlertTriangle,
  History,
  Activity,
} from "lucide-react";
import {
  formatBitsPerSec,
  formatBytes,
  formatTimestamp,
} from "../lib/format";
import type {
  BandwidthSample,
  ConnectionHistoryEvent,
  NetworkTelemetrySnapshot,
} from "../lib/types";
import {
  InterfaceIcon,
  StatusPill,
  interfaceKindColors,
  interfaceKindLabel,
} from "../components/InterfaceBadge";
import clsx from "clsx";

interface MonitorPageProps {
  telemetry: NetworkTelemetrySnapshot;
  history: BandwidthSample[];
  connectionEvents: ConnectionHistoryEvent[];
}

export const MonitorPage: React.FC<MonitorPageProps> = ({
  telemetry,
  history,
  connectionEvents,
}) => {
  const latest = history[history.length - 1];
  const wifiBps = latest?.wifi_download_bps || 0;
  const ethBps = latest?.ethernet_download_bps || 0;
  const usbBps = latest?.usb_download_bps || 0;

  const unstableInterfaces = telemetry.interfaces.filter(
    (i) => i.status === "Unstable" || (i.status === "Connected" && i.packet_loss_pct > 5)
  );

  const stackedData = history.map((h) => ({
    time: h.time_label,
    wifiMbps: Number((h.wifi_download_bps / 1_000_000).toFixed(2)),
    ethMbps: Number((h.ethernet_download_bps / 1_000_000).toFixed(2)),
    usbMbps: Number((h.usb_download_bps / 1_000_000).toFixed(2)),
    totalMbps: Number((h.total_download_bps / 1_000_000).toFixed(2)),
  }));

  return (
    <div className="space-y-5 pb-6">
      {/* Automatic Unstable Network Detection Alert */}
      {unstableInterfaces.length > 0 && (
        <div className="p-4 rounded-2xl bg-amber-500/15 border border-amber-400/35 flex items-start gap-3">
          <AlertTriangle className="w-5 h-5 text-amber-400 shrink-0 mt-0.5" />
          <div className="text-xs text-amber-800 dark:text-amber-200">
            <strong>Unstable Network Detected:</strong>{" "}
            {unstableInterfaces.map((n) => `${n.name} (loss ${n.packet_loss_pct.toFixed(1)}%, stability ${Math.round(n.stability_score)}%)`).join(", ")}.
            NetFusion has automatically reduced segment weight on unstable paths to protect aggregate download speed.
          </div>
        </div>
      )}

      {/* Separate Wi-Fi, Ethernet, and USB Tethering Statistics */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <CategoryStatCard
          icon={<Wifi className="w-5 h-5 text-electric-400" />}
          title="Wi-Fi Adapters"
          speedBps={wifiBps}
          adapters={telemetry.interfaces.filter((i) => i.kind === "WiFi")}
          accent="border-electric-400/30"
        />
        <CategoryStatCard
          icon={<Network className="w-5 h-5 text-lavender-400" />}
          title="Ethernet Adapters"
          speedBps={ethBps}
          adapters={telemetry.interfaces.filter((i) => i.kind === "Ethernet")}
          accent="border-lavender-400/30"
        />
        <CategoryStatCard
          icon={<Smartphone className="w-5 h-5 text-emerald-400" />}
          title="USB Tethering & Mobile"
          speedBps={usbBps}
          adapters={telemetry.interfaces.filter(
            (i) => i.kind === "UsbTethering" || i.kind === "Cellular"
          )}
          accent="border-emerald-400/30"
        />
      </div>

      {/* Total Aggregated Bandwidth Graph Broken Down by Medium */}
      <div className="glass-card rounded-2xl p-5">
        <div className="flex flex-wrap items-center justify-between gap-2 mb-4">
          <div>
            <h2 className="text-sm font-bold text-slate-900 dark:text-white">
              Total Aggregated Bandwidth by Medium (Mbps)
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              Live contribution of Wi-Fi, Ethernet, and USB Tethering to total download throughput
            </p>
          </div>
          <div className="flex items-center gap-4 text-xs">
            <span className="flex items-center gap-1.5 text-electric-400">
              <span className="w-2.5 h-2.5 rounded-full bg-electric-400" /> Wi-Fi
            </span>
            <span className="flex items-center gap-1.5 text-lavender-400">
              <span className="w-2.5 h-2.5 rounded-full bg-lavender-400" /> Ethernet
            </span>
            <span className="flex items-center gap-1.5 text-emerald-400">
              <span className="w-2.5 h-2.5 rounded-full bg-emerald-400" /> USB Tethering
            </span>
          </div>
        </div>

        <div className="h-60">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={stackedData} margin={{ top: 8, right: 8, left: -16, bottom: 0 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="rgba(148,163,184,0.12)" />
              <XAxis dataKey="time" tick={{ fontSize: 11, fill: "#64748b" }} axisLine={false} tickLine={false} />
              <YAxis tick={{ fontSize: 11, fill: "#64748b" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{
                  backgroundColor: "rgba(9, 14, 33, 0.92)",
                  borderColor: "rgba(167, 139, 250, 0.3)",
                  borderRadius: "12px",
                  fontSize: "12px",
                  color: "#f8fafc",
                }}
              />
              <Area type="monotone" dataKey="wifiMbps" stackId="1" name="Wi-Fi (Mbps)" stroke="#38bdf8" fill="#38bdf8" fillOpacity={0.35} isAnimationActive={false} />
              <Area type="monotone" dataKey="ethMbps" stackId="1" name="Ethernet (Mbps)" stroke="#a78bfa" fill="#a78bfa" fillOpacity={0.35} isAnimationActive={false} />
              <Area type="monotone" dataKey="usbMbps" stackId="1" name="USB Tethering (Mbps)" stroke="#34d399" fill="#34d399" fillOpacity={0.35} isAnimationActive={false} />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Per-Interface Live Bandwidth, Ping, Packet Loss & Data Usage Cards */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        {telemetry.interfaces.map((nic) => {
          const colors = interfaceKindColors(nic.kind);
          const nicSeries = history.map((h) => ({
            time: h.time_label,
            dlMbps: Number(((h.per_interface[nic.id]?.download_bps || 0) / 1_000_000).toFixed(2)),
            ping: h.per_interface[nic.id]?.latency_ms ?? null,
          }));

          return (
            <div key={nic.id} className="glass-card rounded-2xl p-5 space-y-3.5">
              <div className="flex items-start justify-between gap-2">
                <div className="flex items-center gap-3 min-w-0">
                  <span className={clsx("p-2.5 rounded-xl border", colors.badge)}>
                    <InterfaceIcon kind={nic.kind} className="w-4 h-4" />
                  </span>
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <h3 className="text-sm font-bold text-slate-900 dark:text-white truncate">
                        {nic.name}
                      </h3>
                      <StatusPill status={nic.status} />
                    </div>
                    <p className="text-[11px] text-slate-500 dark:text-slate-400 truncate">
                      {interfaceKindLabel(nic.kind)} · {nic.ipv4 || "No IP"} · MAC {nic.mac_address}
                    </p>
                  </div>
                </div>
                <div className="text-right tabular-nums shrink-0">
                  <div className="text-sm font-extrabold text-electric-500 dark:text-electric-400">
                    ↓ {formatBitsPerSec(nic.download_bps).full}
                  </div>
                  <div className="text-[11px] text-lavender-500 dark:text-lavender-300">
                    ↑ {formatBitsPerSec(nic.upload_bps).full}
                  </div>
                </div>
              </div>

              {/* Per-NIC Live Bandwidth & Ping Mini Chart */}
              <div className="h-32">
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={nicSeries} margin={{ top: 4, right: 6, left: -22, bottom: 0 }}>
                    <CartesianGrid strokeDasharray="3 3" stroke="rgba(148,163,184,0.1)" />
                    <XAxis dataKey="time" hide />
                    <YAxis tick={{ fontSize: 10, fill: "#64748b" }} axisLine={false} tickLine={false} />
                    <Tooltip
                      contentStyle={{
                        backgroundColor: "rgba(9, 14, 33, 0.92)",
                        borderColor: "rgba(56, 189, 248, 0.3)",
                        borderRadius: "10px",
                        fontSize: "11px",
                        color: "#fff",
                      }}
                    />
                    <Line
                      type="monotone"
                      dataKey="dlMbps"
                      name="Download (Mbps)"
                      stroke={colors.accentHex}
                      strokeWidth={2}
                      dot={false}
                      isAnimationActive={false}
                    />
                  </LineChart>
                </ResponsiveContainer>
              </div>

              {/* Real-Time Ping, Jitter, Packet Loss & Data Usage Footer */}
              <div className="grid grid-cols-4 gap-2 pt-2 border-t border-slate-200/60 dark:border-white/[0.06] text-center tabular-nums">
                <div>
                  <div className="text-[10px] text-slate-500 dark:text-slate-400">Ping / Jitter</div>
                  <div className="text-xs font-bold text-slate-900 dark:text-white">
                    {nic.latency_ms !== null ? `${Math.round(nic.latency_ms)}ms` : "—"} /{" "}
                    {nic.jitter_ms !== null ? `${Math.round(nic.jitter_ms)}ms` : "—"}
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-slate-500 dark:text-slate-400">Packet Loss</div>
                  <div
                    className={clsx(
                      "text-xs font-bold",
                      nic.packet_loss_pct > 2 ? "text-rose-400" : "text-emerald-400"
                    )}
                  >
                    {nic.packet_loss_pct.toFixed(1)}%
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-slate-500 dark:text-slate-400">Today RX</div>
                  <div className="text-xs font-bold text-slate-900 dark:text-white">
                    {formatBytes(nic.rx_bytes_today)}
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-slate-500 dark:text-slate-400">Today TX</div>
                  <div className="text-xs font-bold text-slate-900 dark:text-white">
                    {formatBytes(nic.tx_bytes_today)}
                  </div>
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {/* Connection History Table */}
      <div className="glass-card rounded-2xl p-5 space-y-3">
        <div className="flex items-center gap-2">
          <History className="w-4 h-4 text-lavender-400" />
          <h2 className="text-sm font-bold text-slate-900 dark:text-white">
            Interface Availability & Connection History
          </h2>
        </div>

        {connectionEvents.length === 0 ? (
          <p className="text-xs text-slate-500 dark:text-slate-400 py-3">
            Interface state transitions (connect, disconnect, instability, failover) are logged here in real time.
          </p>
        ) : (
          <div className="max-h-56 overflow-y-auto divide-y divide-slate-200/60 dark:divide-white/[0.06] text-xs">
            {connectionEvents.map((ev) => (
              <div key={ev.id} className="py-2 flex items-center justify-between gap-4">
                <div className="flex items-center gap-2.5 min-w-0">
                  <InterfaceIcon kind={ev.kind} className="w-3.5 h-3.5 text-electric-400 shrink-0" />
                  <span className="font-semibold text-slate-800 dark:text-slate-200">
                    {ev.interface_name}
                  </span>
                  <span className="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-lavender-500/15 text-lavender-400">
                    {ev.event_type}
                  </span>
                  <span className="text-slate-500 dark:text-slate-400 truncate">{ev.details}</span>
                </div>
                <span className="text-[11px] text-slate-400 tabular-nums shrink-0">
                  {formatTimestamp(ev.timestamp_ms)}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

const CategoryStatCard: React.FC<{
  icon: React.ReactNode;
  title: string;
  speedBps: number;
  adapters: NetworkTelemetrySnapshot["interfaces"];
  accent: string;
}> = ({ icon, title, speedBps, adapters, accent }) => {
  const sp = formatBitsPerSec(speedBps);
  const totalToday = adapters.reduce((acc, a) => acc + a.rx_bytes_today + a.tx_bytes_today, 0);
  const connected = adapters.filter((a) => a.status === "Connected" || a.status === "Unstable").length;

  return (
    <div className={clsx("glass-card rounded-2xl p-4 border", accent)}>
      <div className="flex items-center justify-between">
        <span className="text-xs font-bold text-slate-800 dark:text-slate-200">{title}</span>
        <div className="p-2 rounded-xl bg-slate-100 dark:bg-white/[0.05]">{icon}</div>
      </div>
      <div className="mt-2.5 flex items-baseline gap-1.5 tabular-nums">
        <span className="text-2xl font-extrabold text-slate-900 dark:text-white">{sp.value}</span>
        <span className="text-xs font-semibold text-slate-500 dark:text-slate-400">{sp.unit}</span>
      </div>
      <div className="mt-1.5 flex items-center justify-between text-[11px] text-slate-500 dark:text-slate-400">
        <span>
          {connected} / {adapters.length} Connected
        </span>
        <span>Today: {formatBytes(totalToday)}</span>
      </div>
    </div>
  );
};

