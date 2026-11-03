import React, { useState, useEffect } from "react";
import {
  Sparkles,
  Zap,
  Scale,
  ShieldAlert,
  Sliders,
  Server,
  Lock,
  CheckCircle2,
  AlertTriangle,
  PlugZap,
  PowerOff,
  Info,
  Star,
} from "lucide-react";
import clsx from "clsx";
import { formatBitsPerSec, formatBytes } from "../lib/format";
import type {
  BondingMode,
  BondingStateSnapshot,
  NetworkInterfaceInfo,
} from "../lib/types";
import {
  InterfaceIcon,
  StatusPill,
  interfaceKindColors,
  interfaceKindLabel,
} from "../components/InterfaceBadge";

interface BondingPageProps {
  interfaces: NetworkInterfaceInfo[];
  bonding: BondingStateSnapshot;
  onSelectMode: (mode: BondingMode) => void;
  onToggleInterface: (id: string, enabled: boolean) => void;
  onChangeCustomWeight: (id: string, weight: number) => void;
  onSetPrimaryFailover: (id: string) => void;
  onConnectServer: (endpoint: string, authToken?: string | null) => Promise<void>;
  onDisconnectServer: () => Promise<void>;
}

const MODES: {
  id: BondingMode;
  title: string;
  desc: string;
  icon: React.ComponentType<{ className?: string }>;
}[] = [
  {
    id: "Smart",
    title: "Smart Bonding",
    desc: "Automatically distributes traffic using real-time EWMA capacity, RTT latency, and loss avoidance.",
    icon: Sparkles,
  },
  {
    id: "MaxSpeed",
    title: "Maximum Speed",
    desc: "Aggressively saturates every active connection with dynamic work-stealing to maximize peak throughput.",
    icon: Zap,
  },
  {
    id: "Balanced",
    title: "Balanced Mode",
    desc: "Balances download speed, low ping latency, and connection stability to avoid link congestion.",
    icon: Scale,
  },
  {
    id: "Failover",
    title: "Failover Mode",
    desc: "Uses your primary network exclusively and switches instantaneously if disconnect or packet loss occurs.",
    icon: ShieldAlert,
  },
  {
    id: "Custom",
    title: "Custom Mode",
    desc: "Manually configure traffic allocation ratios across your enabled network interfaces.",
    icon: Sliders,
  },
];

export const BondingPage: React.FC<BondingPageProps> = ({
  interfaces,
  bonding,
  onSelectMode,
  onToggleInterface,
  onChangeCustomWeight,
  onSetPrimaryFailover,
  onConnectServer,
  onDisconnectServer,
}) => {
  const [endpoint, setEndpoint] = useState(bonding.server_endpoint || "127.0.0.1:9443");
  const [authToken, setAuthToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    if (bonding.server_endpoint) {
      setEndpoint(bonding.server_endpoint);
    }
  }, [bonding.server_endpoint]);

  const handleConnect = async () => {
    setBusy(true);
    setActionError(null);
    try {
      await onConnectServer(endpoint.trim(), authToken.trim() ? authToken.trim() : null);
      setAuthToken("");
    } catch (err: any) {
      setActionError(typeof err === "string" ? err : err?.message || "Connection failed");
    } finally {
      setBusy(false);
    }
  };

  const handleDisconnect = async () => {
    setBusy(true);
    setActionError(null);
    try {
      await onDisconnectServer();
    } catch (err: any) {
      setActionError(typeof err === "string" ? err : err?.message || "Disconnect failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-5 pb-6">
      {/* Bonding Mode Cards */}
      <div className="glass-card rounded-2xl p-5 space-y-3.5">
        <div>
          <h2 className="text-sm font-bold text-slate-900 dark:text-white">
            Traffic Allocation & Bonding Mode
          </h2>
          <p className="text-xs text-slate-500 dark:text-slate-400">
            Select how NetFusion schedules packets and download segments across active network adapters
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 xl:grid-cols-5 gap-3">
          {MODES.map((m) => {
            const Icon = m.icon;
            const active = bonding.mode === m.id;
            return (
              <button
                key={m.id}
                onClick={() => onSelectMode(m.id)}
                className={clsx(
                  "text-left p-4 rounded-2xl border transition-all flex flex-col justify-between",
                  active
                    ? "bg-gradient-to-br from-electric-500/20 via-indigo-500/15 to-lavender-500/20 border-electric-400/50 shadow-glow-blue"
                    : "bg-slate-100/60 dark:bg-white/[0.03] border-slate-200/70 dark:border-white/[0.07] hover:border-lavender-400/35"
                )}
              >
                <div>
                  <div className="flex items-center justify-between mb-2.5">
                    <div
                      className={clsx(
                        "p-2 rounded-xl",
                        active
                          ? "bg-electric-500/20 text-electric-400"
                          : "bg-slate-200/70 dark:bg-white/5 text-slate-500 dark:text-slate-400"
                      )}
                    >
                      <Icon className="w-4 h-4" />
                    </div>
                    {active && (
                      <span className="text-[10px] font-bold uppercase tracking-wider px-2 py-0.5 rounded-full bg-electric-500/20 text-electric-500 dark:text-electric-300">
                        Active
                      </span>
                    )}
                  </div>
                  <h3 className="text-xs font-bold text-slate-900 dark:text-white">{m.title}</h3>
                  <p className="mt-1 text-[11px] leading-relaxed text-slate-500 dark:text-slate-400">
                    {m.desc}
                  </p>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {/* Multipath Encrypted Bonding Tunnel Server Configuration */}
      <div className="glass-card rounded-2xl p-5 space-y-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="flex items-start gap-3">
            <div className="p-2.5 rounded-xl bg-lavender-500/15 text-lavender-400 border border-lavender-400/30">
              <Server className="w-5 h-5" />
            </div>
            <div>
              <div className="flex items-center gap-2.5">
                <h2 className="text-sm font-bold text-slate-900 dark:text-white">
                  Multipath Encrypted Bonding Server (Tunnel Endpoint)
                </h2>
                <TunnelBadge status={bonding.tunnel_status} />
              </div>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                Establishes ChaCha20-Poly1305 encrypted TCP subflows over every enabled Windows NIC (pinned via{" "}
                <code className="text-[11px] font-mono text-electric-400">IP_UNICAST_IF</code>) and reassembles packets at your VPS bonding endpoint.
              </p>
            </div>
          </div>

          {bonding.local_socks_port && bonding.tunnel_status === "Connected" && (
            <div className="px-3 py-1.5 rounded-xl bg-emerald-500/10 border border-emerald-400/30 text-xs text-emerald-500 dark:text-emerald-300 font-mono">
              Bonded SOCKS5: 127.0.0.1:{bonding.local_socks_port}
            </div>
          )}
        </div>

        {/* Status Alert if Server Unavailable */}
        {(bonding.tunnel_status === "ServerUnreachable" ||
          bonding.tunnel_status === "AuthFailed") && (
          <div className="p-3.5 rounded-xl bg-amber-500/15 border border-amber-400/35 flex items-start gap-2.5 text-xs text-amber-700 dark:text-amber-200">
            <AlertTriangle className="w-4 h-4 shrink-0 mt-0.5 text-amber-400" />
            <div>
              <strong>Bonding Server Unavailable:</strong> {bonding.status_message}. NetFusion has automatically fallen back to{" "}
              <strong>Direct Multi-Path HTTP Range Acceleration</strong> across your enabled Windows interfaces without interrupting downloads.
            </div>
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-12 gap-3 items-end">
          <div className="lg:col-span-5">
            <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1">
              Bonding Server Host : Port
            </label>
            <input
              type="text"
              value={endpoint}
              onChange={(e) => setEndpoint(e.target.value)}
              placeholder="e.g. 203.0.113.42:9443 or 127.0.0.1:9443"
              className="w-full px-3.5 py-2 rounded-xl text-xs font-mono bg-slate-100/80 dark:bg-navy-950/70 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white focus:outline-none focus:border-electric-400"
            />
          </div>

          <div className="lg:col-span-4">
            <label className="flex items-center gap-1.5 text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1">
              <Lock className="w-3 h-3 text-lavender-400" />
              Pre-Shared Auth Key (HMAC-SHA256)
            </label>
            <input
              type="password"
              value={authToken}
              onChange={(e) => setAuthToken(e.target.value)}
              placeholder="Leave blank to use saved key"
              className="w-full px-3.5 py-2 rounded-xl text-xs font-mono bg-slate-100/80 dark:bg-navy-950/70 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white focus:outline-none focus:border-electric-400"
            />
          </div>

          <div className="lg:col-span-3 flex gap-2">
            {bonding.tunnel_status === "Connected" || bonding.tunnel_status === "Connecting" ? (
              <button
                onClick={handleDisconnect}
                disabled={busy}
                className="w-full inline-flex items-center justify-center gap-1.5 px-4 py-2 rounded-xl text-xs font-semibold bg-rose-500/15 text-rose-500 dark:text-rose-300 border border-rose-400/30 hover:bg-rose-500/25"
              >
                <PowerOff className="w-3.5 h-3.5" />
                Disconnect Tunnel
              </button>
            ) : (
              <button
                onClick={handleConnect}
                disabled={busy}
                className="w-full inline-flex items-center justify-center gap-1.5 px-4 py-2 rounded-xl text-xs font-semibold text-white bg-gradient-to-r from-electric-500 via-indigo-500 to-lavender-500 hover:brightness-110 shadow-glow-blue"
              >
                <PlugZap className="w-3.5 h-3.5" />
                Connect Bonding Server
              </button>
            )}
          </div>
        </div>

        {actionError && (
          <div className="text-xs text-rose-400 font-medium">{actionError}</div>
        )}

        {/* Tunnel Telemetry Metrics Strip */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 pt-2">
          <div className="p-3 rounded-xl bg-slate-100/70 dark:bg-white/[0.03] border border-slate-200/60 dark:border-white/[0.06]">
            <div className="text-[11px] text-slate-500 dark:text-slate-400">Active Encrypted Subflows</div>
            <div className="text-base font-bold tabular-nums text-slate-900 dark:text-white mt-0.5">
              {bonding.active_subflows} Subflows
            </div>
          </div>
          <div className="p-3 rounded-xl bg-slate-100/70 dark:bg-white/[0.03] border border-slate-200/60 dark:border-white/[0.06]">
            <div className="text-[11px] text-slate-500 dark:text-slate-400">Tunnel Heartbeat RTT</div>
            <div className="text-base font-bold tabular-nums text-slate-900 dark:text-white mt-0.5">
              {bonding.tunnel_rtt_ms !== null ? `${Math.round(bonding.tunnel_rtt_ms)} ms` : "—"}
            </div>
          </div>
          <div className="p-3 rounded-xl bg-slate-100/70 dark:bg-white/[0.03] border border-slate-200/60 dark:border-white/[0.06]">
            <div className="text-[11px] text-slate-500 dark:text-slate-400">Total Tunnel RX</div>
            <div className="text-base font-bold tabular-nums text-electric-500 dark:text-electric-400 mt-0.5">
              {formatBytes(bonding.total_bonded_rx_bytes)}
            </div>
          </div>
          <div className="p-3 rounded-xl bg-slate-100/70 dark:bg-white/[0.03] border border-slate-200/60 dark:border-white/[0.06]">
            <div className="text-[11px] text-slate-500 dark:text-slate-400">Reassembly Window</div>
            <div className="text-base font-bold tabular-nums text-lavender-500 dark:text-lavender-300 mt-0.5">
              {bonding.reassembly_queue_depth} Frames
            </div>
          </div>
        </div>
      </div>

      {/* Per-Interface Bonding & Traffic Allocation Controls */}
      <div className="glass-card rounded-2xl p-5 space-y-3.5">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 className="text-sm font-bold text-slate-900 dark:text-white">
              Detected Windows Network Adapters ({interfaces.length})
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              Real-time adapter capabilities, per-socket IP_UNICAST_IF binding status, and dynamic allocation share
            </p>
          </div>
          <span className="inline-flex items-center gap-1.5 text-[11px] text-slate-500 dark:text-slate-400">
            <Info className="w-3.5 h-3.5 text-electric-400" />
            Non-intrusive per-socket routing — Windows system routing table is never altered
          </span>
        </div>

        <div className="space-y-3">
          {interfaces.map((nic) => {
            const colors = interfaceKindColors(nic.kind);
            return (
              <div
                key={nic.id}
                className="p-4 rounded-2xl bg-slate-100/70 dark:bg-white/[0.035] border border-slate-200/70 dark:border-white/[0.08] space-y-3"
              >
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="flex items-center gap-3 min-w-0">
                    <span className={clsx("p-2.5 rounded-xl border", colors.badge)}>
                      <InterfaceIcon kind={nic.kind} className="w-5 h-5" />
                    </span>
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="text-sm font-bold text-slate-900 dark:text-white">
                          {nic.name}
                        </span>
                        <span className="text-xs text-slate-500 dark:text-slate-400">
                          ({nic.description})
                        </span>
                        <StatusPill status={nic.status} />
                        {nic.subflow_connected && (
                          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-500/15 text-emerald-400 border border-emerald-400/30">
                            <CheckCircle2 className="w-3 h-3" />
                            Tunnel Subflow Up
                          </span>
                        )}
                      </div>
                      <div className="mt-0.5 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-slate-500 dark:text-slate-400 tabular-nums">
                        <span>Type: {interfaceKindLabel(nic.kind)}</span>
                        <span>IPv4: {nic.ipv4 || "—"}</span>
                        <span>Gateway: {nic.gateway || "—"}</span>
                        <span>Link: {formatBitsPerSec(nic.link_speed_bps).full}</span>
                        <span>
                          Latency:{" "}
                          {nic.latency_ms !== null ? `${Math.round(nic.latency_ms)} ms` : "—"}
                        </span>
                        <span>Loss: {nic.packet_loss_pct.toFixed(1)}%</span>
                        <span>Stability: {Math.round(nic.stability_score)}%</span>
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center gap-3">
                    {bonding.mode === "Failover" && (
                      <button
                        onClick={() => onSetPrimaryFailover(nic.id)}
                        className={clsx(
                          "inline-flex items-center gap-1 px-2.5 py-1 rounded-xl text-xs font-semibold border transition-colors",
                          nic.is_primary_failover
                            ? "bg-amber-500/20 text-amber-400 border-amber-400/40"
                            : "glass-card text-slate-500 dark:text-slate-400"
                        )}
                      >
                        <Star className="w-3.5 h-3.5" />
                        {nic.is_primary_failover ? "Primary Link" : "Set Primary"}
                      </button>
                    )}

                    <label className="inline-flex items-center gap-2 cursor-pointer">
                      <span className="text-xs font-medium text-slate-600 dark:text-slate-300">
                        {nic.enabled_for_bonding ? "Enabled" : "Disabled"}
                      </span>
                      <input
                        type="checkbox"
                        checked={nic.enabled_for_bonding}
                        onChange={(e) => onToggleInterface(nic.id, e.target.checked)}
                        className="sr-only peer"
                      />
                      <div className="relative w-10 h-5 bg-slate-300 dark:bg-slate-700 rounded-full peer peer-checked:bg-gradient-to-r peer-checked:from-electric-500 peer-checked:to-lavender-500 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:after:translate-x-5" />
                    </label>
                  </div>
                </div>

                {/* Live Traffic Share Bar + Custom Weight Slider */}
                <div className="space-y-1.5">
                  <div className="flex items-center justify-between text-xs tabular-nums">
                    <span className="text-slate-500 dark:text-slate-400">
                      Current Traffic Allocation Share:{" "}
                      <strong className="text-slate-800 dark:text-white">
                        {nic.allocated_weight_pct.toFixed(1)}%
                      </strong>
                    </span>
                    <span className="text-electric-600 dark:text-electric-400 font-semibold">
                      ↓ {formatBitsPerSec(nic.download_bps).full} · ↑{" "}
                      {formatBitsPerSec(nic.upload_bps).full}
                    </span>
                  </div>
                  <div className="h-2 rounded-full bg-slate-200 dark:bg-navy-950 overflow-hidden">
                    <div
                      className="h-full rounded-full bg-gradient-to-r from-electric-400 to-lavender-400 transition-all duration-300"
                      style={{ width: `${Math.min(100, nic.allocated_weight_pct)}%` }}
                    />
                  </div>

                  {bonding.mode === "Custom" && nic.enabled_for_bonding && (
                    <div className="pt-2 flex items-center gap-3">
                      <span className="text-xs font-medium text-lavender-400 w-36">
                        Manual Weight: {nic.custom_weight}
                      </span>
                      <input
                        type="range"
                        min={5}
                        max={100}
                        step={5}
                        value={nic.custom_weight}
                        onChange={(e) =>
                          onChangeCustomWeight(nic.id, Number(e.target.value))
                        }
                        className="flex-1 accent-lavender-500"
                      />
                    </div>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
};

const TunnelBadge: React.FC<{ status: BondingStateSnapshot["tunnel_status"] }> = ({
  status,
}) => {
  const styles = {
    Connected: "bg-emerald-500/15 text-emerald-400 border-emerald-400/30",
    Connecting: "bg-electric-500/15 text-electric-400 border-electric-400/30",
    Degraded: "bg-amber-500/15 text-amber-400 border-amber-400/30",
    ServerUnreachable: "bg-rose-500/15 text-rose-400 border-rose-400/30",
    AuthFailed: "bg-rose-500/15 text-rose-400 border-rose-400/30",
    Disconnected: "bg-slate-500/15 text-slate-400 border-slate-400/30",
  }[status];

  return (
    <span className={clsx("px-2.5 py-0.5 rounded-full text-[11px] font-semibold border", styles)}>
      {status === "ServerUnreachable" ? "Server Unavailable" : status}
    </span>
  );
};

