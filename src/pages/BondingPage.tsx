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
  Power,
  Wifi,
  RefreshCw,
  Globe,
  Signal,
} from "lucide-react";
import clsx from "clsx";
import { formatBitsPerSec, formatBytes } from "../lib/format";
import type {
  BondingMode,
  BondingStateSnapshot,
  NetworkInterfaceInfo,
  WifiNetworkScanItem,
} from "../lib/types";
import {
  InterfaceIcon,
  StatusPill,
  interfaceKindColors,
  interfaceKindLabel,
} from "../components/InterfaceBadge";
import { api } from "../lib/tauri";

interface BondingPageProps {
  interfaces: NetworkInterfaceInfo[];
  bonding: BondingStateSnapshot;
  onSelectMode: (mode: BondingMode) => void;
  onToggleInterface: (id: string, enabled: boolean) => void;
  onChangeCustomWeight: (id: string, weight: number) => void;
  onSetPrimaryFailover: (id: string) => void;
  onSetBondingActive: (active: boolean) => void;
  onSetAutoBonding: (enabled: boolean) => void;
  onSetAdaptiveLoadBalancing: (enabled: boolean) => void;
  onSetSystemProxyRouting: (enabled: boolean) => Promise<void>;
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
    title: "Adaptive Bonding",
    desc: "Automatically distributes traffic based on real-time bandwidth, RTT latency, and packet loss.",
    icon: Sparkles,
  },
  {
    id: "MaxSpeed",
    title: "Speed Priority",
    desc: "Uses the fastest available connections aggressively with dynamic work-stealing for peak throughput.",
    icon: Zap,
  },
  {
    id: "Balanced",
    title: "Stability Priority",
    desc: "Prioritizes connections with the lowest packet loss, low jitter, and highest link reliability.",
    icon: Scale,
  },
  {
    id: "Failover",
    title: "Failover Mode",
    desc: "Routes traffic through your primary connection and switches instantaneously if it fails or drops packets.",
    icon: ShieldAlert,
  },
  {
    id: "Custom",
    title: "Download Acceleration",
    desc: "Splits supported downloads across multiple connections with customizable per-interface traffic weights.",
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
  onSetBondingActive,
  onSetAutoBonding,
  onSetAdaptiveLoadBalancing,
  onSetSystemProxyRouting,
  onConnectServer,
  onDisconnectServer,
}) => {
  const [endpoint, setEndpoint] = useState(bonding.server_endpoint || "127.0.0.1:9443");
  const [authToken, setAuthToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  // Multi-Wi-Fi Scanner & Connector state per Wi-Fi adapter
  const [scanningAdapter, setScanningAdapter] = useState<string | null>(null);
  const [wifiScans, setWifiScans] = useState<Record<string, WifiNetworkScanItem[]>>({});
  const [selectedSsid, setSelectedSsid] = useState<Record<string, string>>({});
  const [wifiPassword, setWifiPassword] = useState<Record<string, string>>({});
  const [wifiBusyAdapter, setWifiBusyAdapter] = useState<string | null>(null);
  const [wifiFeedback, setWifiFeedback] = useState<Record<string, string>>({});

  useEffect(() => {
    if (bonding.server_endpoint) {
      setEndpoint(bonding.server_endpoint);
    }
  }, [bonding.server_endpoint]);

  const bondingActive = bonding.bonding_active !== false;
  const autoBonding = bonding.auto_bonding !== false;
  const adaptiveLb = bonding.adaptive_load_balancing !== false;
  const sysProxyRouted = bonding.system_proxy_routed === true;
  const wifiAdaptersCount = interfaces.filter((i) => i.kind === "WiFi").length;

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

  const handleScanWifi = async (adapterName: string) => {
    setScanningAdapter(adapterName);
    setWifiFeedback((prev) => ({ ...prev, [adapterName]: "" }));
    try {
      const list = await api.scanWifiNetworks(adapterName);
      setWifiScans((prev) => ({ ...prev, [adapterName]: list }));
    } catch (err: any) {
      setWifiFeedback((prev) => ({
        ...prev,
        [adapterName]: typeof err === "string" ? err : "Failed to scan Wi-Fi networks",
      }));
    } finally {
      setScanningAdapter(null);
    }
  };

  const handleConnectWifi = async (adapterName: string, ssid: string) => {
    setWifiBusyAdapter(adapterName);
    setWifiFeedback((prev) => ({ ...prev, [adapterName]: "" }));
    try {
      const pwd = wifiPassword[adapterName]?.trim() || null;
      const msg = await api.connectWifiNetwork(adapterName, ssid, pwd);
      setWifiFeedback((prev) => ({ ...prev, [adapterName]: msg }));
      setWifiPassword((prev) => ({ ...prev, [adapterName]: "" }));
      setTimeout(() => handleScanWifi(adapterName), 1500);
    } catch (err: any) {
      setWifiFeedback((prev) => ({
        ...prev,
        [adapterName]: typeof err === "string" ? err : "Wi-Fi connection failed",
      }));
    } finally {
      setWifiBusyAdapter(null);
    }
  };

  return (
    <div className="space-y-5 pb-6">
      {/* Master Bonding Controls & Virtual Network Interface Card */}
      <div className="glass-card rounded-2xl p-5 space-y-4 border border-electric-400/30">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-sm font-bold text-slate-900 dark:text-white">
              Bonding Engine Controls & Unified Virtual Network Interface
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              Start or stop multi-network bonding, configure automatic connection aggregation, and route Windows app traffic through the Virtual Bonding Gateway
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-2.5">
            {bondingActive ? (
              <button
                onClick={() => onSetBondingActive(false)}
                className="inline-flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold bg-rose-500/15 text-rose-500 dark:text-rose-300 border border-rose-400/35 hover:bg-rose-500/25"
              >
                <Power className="w-3.5 h-3.5" />
                Stop Bonding
              </button>
            ) : (
              <button
                onClick={() => onSetBondingActive(true)}
                className="inline-flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold text-white bg-gradient-to-r from-electric-500 via-indigo-500 to-lavender-500 shadow-glow-blue"
              >
                <Power className="w-3.5 h-3.5" />
                Start Bonding
              </button>
            )}
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          {/* Auto Bonding Toggle */}
          <div className="p-3.5 rounded-xl bg-slate-100/70 dark:bg-white/[0.03] border border-slate-200/70 dark:border-white/[0.07] flex items-center justify-between gap-3">
            <div>
              <div className="text-xs font-bold text-slate-900 dark:text-white">
                Auto Bonding & Reconnect
              </div>
              <div className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">
                Automatically bond new Wi-Fi / USB Tethering links and auto-reconnect dropped Wi-Fi
              </div>
            </div>
            <label className="relative inline-flex items-center cursor-pointer shrink-0">
              <input
                type="checkbox"
                checked={autoBonding}
                onChange={(e) => onSetAutoBonding(e.target.checked)}
                className="sr-only peer"
              />
              <div className="w-10 h-5 bg-slate-300 dark:bg-slate-700 rounded-full peer peer-checked:bg-gradient-to-r peer-checked:from-electric-500 peer-checked:to-lavender-500 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:after:translate-x-5" />
            </label>
          </div>

          {/* Adaptive Load Balancing Toggle */}
          <div className="p-3.5 rounded-xl bg-slate-100/70 dark:bg-white/[0.03] border border-slate-200/70 dark:border-white/[0.07] flex items-center justify-between gap-3">
            <div>
              <div className="text-xs font-bold text-slate-900 dark:text-white">
                Adaptive Load Balancing
              </div>
              <div className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">
                Dynamically scale segment concurrency and prevent overloading slower connections
              </div>
            </div>
            <label className="relative inline-flex items-center cursor-pointer shrink-0">
              <input
                type="checkbox"
                checked={adaptiveLb}
                onChange={(e) => onSetAdaptiveLoadBalancing(e.target.checked)}
                className="sr-only peer"
              />
              <div className="w-10 h-5 bg-slate-300 dark:bg-slate-700 rounded-full peer peer-checked:bg-gradient-to-r peer-checked:from-electric-500 peer-checked:to-lavender-500 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:after:translate-x-5" />
            </label>
          </div>

          {/* Unified Virtual Bonding Interface / System App Routing */}
          <div className="p-3.5 rounded-xl bg-slate-100/70 dark:bg-white/[0.03] border border-slate-200/70 dark:border-white/[0.07] flex items-center justify-between gap-3">
            <div className="min-w-0">
              <div className="flex items-center gap-1.5 text-xs font-bold text-slate-900 dark:text-white">
                <Globe className="w-3.5 h-3.5 text-electric-400 shrink-0" />
                <span className="truncate">Virtual Bonding Gateway</span>
              </div>
              <div className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">
                Route Windows apps via{" "}
                <code className="font-mono text-electric-400">
                  127.0.0.1:{bonding.local_socks_port || 10808}
                </code>{" "}
                (SOCKS5 + HTTP)
              </div>
            </div>
            <label className="relative inline-flex items-center cursor-pointer shrink-0">
              <input
                type="checkbox"
                checked={sysProxyRouted}
                onChange={async (e) => {
                  try {
                    await onSetSystemProxyRouting(e.target.checked);
                  } catch (err: any) {
                    setActionError(typeof err === "string" ? err : "Failed to toggle system routing");
                  }
                }}
                className="sr-only peer"
              />
              <div className="w-10 h-5 bg-slate-300 dark:bg-slate-700 rounded-full peer peer-checked:bg-gradient-to-r peer-checked:from-emerald-500 peer-checked:to-teal-500 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:after:translate-x-5" />
            </label>
          </div>
        </div>
      </div>

      {/* Traffic Distribution Modes (All 5 Modes) */}
      <div className="glass-card rounded-2xl p-5 space-y-3.5">
        <div>
          <h2 className="text-sm font-bold text-slate-900 dark:text-white">
            Traffic Distribution Mode
          </h2>
          <p className="text-xs text-slate-500 dark:text-slate-400">
            Select how NetFusion distributes packets, TCP streams, and parallel download chunks across your connections
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
                  Multipath Encrypted Bonding Server (Single-Connection Packet Aggregation)
                </h2>
                <TunnelBadge status={bonding.tunnel_status} />
              </div>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                Establishes ChaCha20-Poly1305 encrypted subflows over every enabled Windows NIC (pinned via{" "}
                <code className="text-[11px] font-mono text-electric-400">IP_UNICAST_IF</code>) and reassembles packets in sequence at your remote VPS server.
              </p>
            </div>
          </div>

          {bonding.local_socks_port && (
            <div className="px-3 py-1.5 rounded-xl bg-emerald-500/10 border border-emerald-400/30 text-xs text-emerald-500 dark:text-emerald-300 font-mono">
              Virtual Gateway: 127.0.0.1:{bonding.local_socks_port}
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
              <strong>Direct Multi-Path Acceleration</strong> across your enabled Windows interfaces without interrupting traffic.
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

        {actionError && <div className="text-xs text-rose-400 font-medium">{actionError}</div>}

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
            <div className="text-[11px] text-slate-500 dark:text-slate-400">Total Gateway / Tunnel RX</div>
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

      {/* Per-Interface Bonding, Multi-Wi-Fi Scanner & Traffic Allocation Controls */}
      <div className="glass-card rounded-2xl p-5 space-y-3.5">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 className="text-sm font-bold text-slate-900 dark:text-white">
              Network Connections — Wi-Fi 1, Wi-Fi 2, Ethernet & USB Tethering ({interfaces.length})
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              Combine multiple Wi-Fi adapters, Gigabit Ethernet, Android/iPhone USB Tethering, and mobile hotspots simultaneously
            </p>
          </div>
          <span className="inline-flex items-center gap-1.5 text-[11px] text-slate-500 dark:text-slate-400">
            <Info className="w-3.5 h-3.5 text-electric-400" />
            Per-socket IP_UNICAST_IF binding — Windows routing table is never modified
          </span>
        </div>

        {wifiAdaptersCount === 1 && (
          <div className="p-3 rounded-xl bg-electric-500/10 border border-electric-400/25 flex items-center justify-between gap-3 text-xs text-slate-700 dark:text-slate-200">
            <div className="flex items-center gap-2">
              <Wifi className="w-4 h-4 text-electric-400 shrink-0" />
              <span>
                <strong>Multi-Wi-Fi Ready:</strong> 1 physical Wi-Fi adapter detected (`Wi-Fi 1`). Plug in any USB Wi-Fi adapter to automatically unlock `Wi-Fi 2` and connect to two different routers or hotspots simultaneously!
              </span>
            </div>
          </div>
        )}

        <div className="space-y-3">
          {interfaces.map((nic) => {
            const colors = interfaceKindColors(nic.kind);
            const peakDl = formatBitsPerSec(nic.peak_download_bps || 0);
            const peakUl = formatBitsPerSec(nic.peak_upload_bps || 0);
            const scanResults = wifiScans[nic.name];

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
                        {nic.slot_label && (
                          <span className="px-2 py-0.5 rounded-md text-[10px] font-bold bg-electric-500/15 text-electric-500 dark:text-electric-300 border border-electric-400/30">
                            {nic.slot_label}
                          </span>
                        )}
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

                      <div className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-slate-500 dark:text-slate-400 tabular-nums">
                        <span>Type: {interfaceKindLabel(nic.kind)}</span>
                        {nic.kind === "WiFi" && (
                          <>
                            <span className="text-electric-500 dark:text-electric-300 font-semibold">
                              SSID: {nic.wifi_ssid || "Not connected"}
                            </span>
                            {nic.wifi_signal_pct != null && (
                              <span className="inline-flex items-center gap-1 text-emerald-500 dark:text-emerald-400 font-semibold">
                                <Signal className="w-3 h-3" />
                                Signal: {nic.wifi_signal_pct}%
                              </span>
                            )}
                            {nic.wifi_radio_type && <span>Radio: {nic.wifi_radio_type}</span>}
                          </>
                        )}
                        <span>IPv4: {nic.ipv4 || "—"}</span>
                        <span>Gateway: {nic.gateway || "—"}</span>
                        <span>Link Cap: {formatBitsPerSec(nic.link_speed_bps).full}</span>
                        <span className="text-emerald-600 dark:text-emerald-400 font-medium">
                          Max Measured: ↓ {peakDl.full} / ↑ {peakUl.full}
                        </span>
                        <span>
                          Ping: {nic.latency_ms !== null ? `${Math.round(nic.latency_ms)} ms` : "—"}
                        </span>
                        <span>Loss: {nic.packet_loss_pct.toFixed(1)}%</span>
                        <span>Stability: {Math.round(nic.stability_score)}%</span>
                      </div>
                    </div>
                  </div>

                  <div className="flex flex-wrap items-center gap-2.5">
                    {nic.kind === "WiFi" && (
                      <button
                        onClick={() => handleScanWifi(nic.name)}
                        disabled={scanningAdapter === nic.name}
                        className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-xl text-xs font-semibold bg-electric-500/15 text-electric-500 dark:text-electric-300 border border-electric-400/30 hover:bg-electric-500/25"
                      >
                        <RefreshCw
                          className={clsx(
                            "w-3.5 h-3.5",
                            scanningAdapter === nic.name && "animate-spin"
                          )}
                        />
                        {scanResults ? "Refresh Wi-Fi List" : "Select Wi-Fi Network"}
                      </button>
                    )}

                    <button
                      onClick={() => onSetPrimaryFailover(nic.id)}
                      className={clsx(
                        "inline-flex items-center gap-1 px-2.5 py-1 rounded-xl text-xs font-semibold border transition-colors",
                        nic.is_primary_failover
                          ? "bg-amber-500/20 text-amber-400 border-amber-400/40"
                          : "glass-card text-slate-500 dark:text-slate-400"
                      )}
                      title="Set as Primary Priority Connection"
                    >
                      <Star className="w-3.5 h-3.5" />
                      {nic.is_primary_failover ? "Primary Priority" : "Set Primary"}
                    </button>

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

                {/* Per-Adapter Wi-Fi Network Picker (for connecting Wi-Fi 1 & Wi-Fi 2 to separate routers) */}
                {nic.kind === "WiFi" && scanResults && (
                  <div className="pt-3 border-t border-slate-200/70 dark:border-white/10 space-y-2.5">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-bold text-slate-800 dark:text-slate-200">
                        Available Wi-Fi Networks on {nic.slot_label || nic.name} ({scanResults.length})
                      </span>
                      <button
                        onClick={() =>
                          setWifiScans((prev) => {
                            const copy = { ...prev };
                            delete copy[nic.name];
                            return copy;
                          })
                        }
                        className="text-[11px] text-slate-400 hover:text-slate-200"
                      >
                        Hide
                      </button>
                    </div>

                    {wifiFeedback[nic.name] && (
                      <div className="text-xs text-electric-400 font-medium">
                        {wifiFeedback[nic.name]}
                      </div>
                    )}

                    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2 max-h-52 overflow-y-auto pr-1">
                      {scanResults.map((net) => {
                        const isPicked = selectedSsid[nic.name] === net.ssid;
                        return (
                          <div
                            key={`${net.ssid}-${net.bssid}`}
                            className={clsx(
                              "p-2.5 rounded-xl border text-xs flex flex-col justify-between gap-2",
                              net.connected
                                ? "bg-emerald-500/10 border-emerald-400/35"
                                : "bg-white/60 dark:bg-navy-950/70 border-slate-200/70 dark:border-white/[0.07]"
                            )}
                          >
                            <div className="flex items-start justify-between gap-2">
                              <div className="min-w-0">
                                <div className="font-bold text-slate-900 dark:text-white truncate">
                                  {net.ssid}
                                </div>
                                <div className="text-[10px] text-slate-500 dark:text-slate-400">
                                  Signal: <strong>{net.signal_pct}%</strong> · {net.radio_type || "802.11"} · Ch {net.channel || "Auto"}
                                </div>
                              </div>
                              {net.connected ? (
                                <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-500/20 text-emerald-400">
                                  Connected
                                </span>
                              ) : (
                                <button
                                  onClick={() => {
                                    if (net.has_saved_profile && !isPicked) {
                                      handleConnectWifi(nic.name, net.ssid);
                                    } else {
                                      setSelectedSsid((prev) => ({
                                        ...prev,
                                        [nic.name]: isPicked ? "" : net.ssid,
                                      }));
                                    }
                                  }}
                                  disabled={wifiBusyAdapter === nic.name}
                                  className="px-2.5 py-1 rounded-lg text-[11px] font-semibold bg-electric-500/20 text-electric-500 dark:text-electric-300 border border-electric-400/30 hover:bg-electric-500/30"
                                >
                                  {net.has_saved_profile ? "Connect" : "Select"}
                                </button>
                              )}
                            </div>

                            {isPicked && !net.connected && (
                              <div className="flex items-center gap-1.5 pt-1">
                                <input
                                  type="password"
                                  value={wifiPassword[nic.name] || ""}
                                  onChange={(e) =>
                                    setWifiPassword((prev) => ({
                                      ...prev,
                                      [nic.name]: e.target.value,
                                    }))
                                  }
                                  placeholder="Wi-Fi Password (if required)"
                                  className="flex-1 px-2 py-1 rounded-lg text-[11px] bg-slate-100 dark:bg-navy-900 border border-slate-300/70 dark:border-white/10"
                                />
                                <button
                                  onClick={() => handleConnectWifi(nic.name, net.ssid)}
                                  disabled={wifiBusyAdapter === nic.name}
                                  className="px-2.5 py-1 rounded-lg text-[11px] font-bold text-white bg-electric-500 hover:bg-electric-600"
                                >
                                  Join
                                </button>
                              </div>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                )}
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
