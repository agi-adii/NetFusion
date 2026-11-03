import React, { useState } from "react";
import { motion } from "framer-motion";
import {
  Play,
  Square,
  ArrowDownToLine,
  ArrowUpFromLine,
  Activity,
  Waves,
  AlertOctagon,
  History,
  GitMerge,
} from "lucide-react";
import {
  formatBitsPerSec,
  formatByteSpeed,
  formatTimestamp,
} from "../lib/format";
import type {
  NetworkInterfaceInfo,
  SpeedTestProgress,
  SpeedTestRecord,
} from "../lib/types";

interface SpeedTestPageProps {
  interfaces: NetworkInterfaceInfo[];
  progress: SpeedTestProgress;
  records: SpeedTestRecord[];
  onStartTest: (interfaceId: string | null) => Promise<void>;
  onCancelTest: () => Promise<void>;
}

export const SpeedTestPage: React.FC<SpeedTestPageProps> = ({
  interfaces,
  progress,
  records,
  onStartTest,
  onCancelTest,
}) => {
  const [selectedTarget, setSelectedTarget] = useState<string>("BONDED_ALL");
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isBusy =
    running ||
    progress.phase === "Ping" ||
    progress.phase === "Download" ||
    progress.phase === "Upload";

  const handleRun = async () => {
    setError(null);
    setRunning(true);
    try {
      const targetId = selectedTarget === "BONDED_ALL" ? null : selectedTarget;
      await onStartTest(targetId);
    } catch (err: any) {
      setError(typeof err === "string" ? err : err?.message || "Speed test failed");
    } finally {
      setRunning(false);
    }
  };

  const liveSpeed = formatBitsPerSec(
    isBusy
      ? progress.current_bps
      : progress.download_bps > 0
      ? progress.download_bps
      : 0
  );

  const dlFinal = formatBitsPerSec(progress.download_bps);
  const ulFinal = formatBitsPerSec(progress.upload_bps);

  return (
    <div className="space-y-5 pb-6">
      {/* Main Speed Test Arena */}
      <div className="glass-card rounded-2xl p-6 space-y-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <h2 className="text-base font-bold text-slate-900 dark:text-white">
              Real-Network Throughput & Latency Benchmark
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              Measures real payload transfer against Cloudflare Edge / Bonding Server using interface-pinned sockets
            </p>
          </div>

          {/* Target Interface / Aggregate Selector */}
          <div className="flex flex-wrap items-center gap-2.5">
            <select
              disabled={isBusy}
              value={selectedTarget}
              onChange={(e) => setSelectedTarget(e.target.value)}
              className="px-3.5 py-2 rounded-xl text-xs font-semibold bg-slate-100 dark:bg-navy-950 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white"
            >
              <option value="BONDED_ALL">
                Aggregate Bonding (All Enabled Interfaces Combined)
              </option>
              {interfaces
                .filter((n) => n.status !== "Disconnected")
                .map((nic) => (
                  <option key={nic.id} value={nic.id}>
                    Single Interface: {nic.name} ({nic.ipv4 || nic.kind})
                  </option>
                ))}
            </select>

            {isBusy ? (
              <button
                onClick={onCancelTest}
                className="inline-flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-semibold bg-rose-500/20 text-rose-400 border border-rose-400/35 hover:bg-rose-500/30"
              >
                <Square className="w-3.5 h-3.5" />
                Stop Test
              </button>
            ) : (
              <button
                onClick={handleRun}
                className="inline-flex items-center gap-2 px-5 py-2 rounded-xl text-xs font-semibold text-white bg-gradient-to-r from-electric-500 via-indigo-500 to-lavender-500 hover:brightness-110 shadow-glow-blue"
              >
                <Play className="w-3.5 h-3.5" />
                Start Speed Test
              </button>
            )}
          </div>
        </div>

        {/* Center Gauge + 5 Live Metrics */}
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 items-center">
          <div className="lg:col-span-5 flex flex-col items-center justify-center">
            <div className="relative w-56 h-56 rounded-full flex flex-col items-center justify-center border-4 border-electric-400/30 bg-gradient-to-b from-electric-500/15 via-indigo-500/5 to-lavender-500/15 shadow-glow-blue">
              <span className="text-[11px] font-bold uppercase tracking-widest text-lavender-400">
                {progress.phase === "Idle" ? "READY" : progress.phase.toUpperCase()}
              </span>
              <motion.span
                key={liveSpeed.value}
                initial={{ scale: 0.97 }}
                animate={{ scale: 1 }}
                className="text-5xl font-black tabular-nums bg-gradient-to-r from-electric-400 via-white to-lavender-300 bg-clip-text text-transparent my-1"
              >
                {liveSpeed.value}
              </motion.span>
              <span className="text-xs font-semibold text-electric-400">{liveSpeed.unit}</span>
              <span className="mt-1 text-[11px] text-slate-500 dark:text-slate-400">
                {progress.target_label || (selectedTarget === "BONDED_ALL" ? "Bonded Multi-Path" : "Single NIC")}
              </span>
            </div>

            {isBusy && (
              <div className="w-full max-w-xs mt-4 space-y-1">
                <div className="flex justify-between text-[11px] text-slate-400">
                  <span>{progress.message}</span>
                  <span>{Math.round(progress.progress_pct)}%</span>
                </div>
                <div className="h-2 rounded-full bg-slate-200 dark:bg-navy-950 overflow-hidden">
                  <div
                    className="h-full rounded-full bg-gradient-to-r from-electric-400 to-lavender-400 transition-all duration-200"
                    style={{ width: `${progress.progress_pct}%` }}
                  />
                </div>
              </div>
            )}
          </div>

          <div className="lg:col-span-7 grid grid-cols-2 sm:grid-cols-3 gap-3.5">
            <MetricBox
              icon={<ArrowDownToLine className="w-4 h-4 text-electric-400" />}
              label="Download Speed"
              value={dlFinal.value}
              unit={dlFinal.unit}
              sub={formatByteSpeed(progress.download_bps)}
            />
            <MetricBox
              icon={<ArrowUpFromLine className="w-4 h-4 text-lavender-400" />}
              label="Upload Speed"
              value={ulFinal.value}
              unit={ulFinal.unit}
              sub={formatByteSpeed(progress.upload_bps)}
            />
            <MetricBox
              icon={<Activity className="w-4 h-4 text-emerald-400" />}
              label="Ping Latency"
              value={progress.ping_ms !== null ? progress.ping_ms.toFixed(1) : "—"}
              unit="ms"
              sub="Minimum RTT"
            />
            <MetricBox
              icon={<Waves className="w-4 h-4 text-amber-400" />}
              label="Jitter"
              value={progress.jitter_ms !== null ? progress.jitter_ms.toFixed(1) : "—"}
              unit="ms"
              sub="RTT variance"
            />
            <MetricBox
              icon={<AlertOctagon className="w-4 h-4 text-rose-400" />}
              label="Packet Loss"
              value={progress.packet_loss_pct.toFixed(1)}
              unit="%"
              sub="Probe loss rate"
            />
            <MetricBox
              icon={<GitMerge className="w-4 h-4 text-sky-400" />}
              label="Test Path"
              value={selectedTarget === "BONDED_ALL" ? "Bonded" : "Pinned"}
              unit=""
              sub={selectedTarget === "BONDED_ALL" ? "All active NICs" : "IP_UNICAST_IF"}
            />
          </div>
        </div>

        {error && (
          <div className="p-3 rounded-xl bg-rose-500/15 border border-rose-400/30 text-xs text-rose-400">
            {error}
          </div>
        )}
      </div>

      {/* Historical Speed Test Records */}
      <div className="glass-card rounded-2xl p-5 space-y-3">
        <div className="flex items-center gap-2">
          <History className="w-4 h-4 text-electric-400" />
          <h2 className="text-sm font-bold text-slate-900 dark:text-white">
            Historical Speed Test Records ({records.length})
          </h2>
        </div>

        {records.length === 0 ? (
          <p className="text-xs text-slate-500 dark:text-slate-400 py-4">
            Run an Aggregate Bonding or Single-Interface speed test to record real benchmark results.
          </p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead>
                <tr className="border-b border-slate-200/70 dark:border-white/10 text-slate-500 dark:text-slate-400">
                  <th className="py-2 pr-4">Date & Time</th>
                  <th className="py-2 pr-4">Mode</th>
                  <th className="py-2 pr-4">Interface(s)</th>
                  <th className="py-2 pr-4">Download</th>
                  <th className="py-2 pr-4">Upload</th>
                  <th className="py-2 pr-4">Ping</th>
                  <th className="py-2 pr-4">Jitter</th>
                  <th className="py-2">Loss</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-200/60 dark:divide-white/[0.06] tabular-nums">
                {records.map((r) => (
                  <tr key={r.id}>
                    <td className="py-2.5 pr-4 text-slate-500 dark:text-slate-400">
                      {formatTimestamp(r.timestamp_ms)}
                    </td>
                    <td className="py-2.5 pr-4 font-semibold text-lavender-500 dark:text-lavender-300">
                      {r.mode_label}
                    </td>
                    <td className="py-2.5 pr-4 text-slate-800 dark:text-slate-200">
                      {r.interface_name}
                    </td>
                    <td className="py-2.5 pr-4 font-bold text-electric-500 dark:text-electric-400">
                      {formatBitsPerSec(r.download_bps).full}
                    </td>
                    <td className="py-2.5 pr-4 font-semibold text-lavender-400">
                      {formatBitsPerSec(r.upload_bps).full}
                    </td>
                    <td className="py-2.5 pr-4">{r.ping_ms.toFixed(1)} ms</td>
                    <td className="py-2.5 pr-4">{r.jitter_ms.toFixed(1)} ms</td>
                    <td className="py-2.5">{r.packet_loss_pct.toFixed(1)}%</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};

const MetricBox: React.FC<{
  icon: React.ReactNode;
  label: string;
  value: string;
  unit: string;
  sub: string;
}> = ({ icon, label, value, unit, sub }) => (
  <div className="p-4 rounded-2xl bg-slate-100/70 dark:bg-white/[0.035] border border-slate-200/70 dark:border-white/[0.07]">
    <div className="flex items-center justify-between">
      <span className="text-xs text-slate-500 dark:text-slate-400">{label}</span>
      {icon}
    </div>
    <div className="mt-2 flex items-baseline gap-1 tabular-nums">
      <span className="text-2xl font-extrabold text-slate-900 dark:text-white">{value}</span>
      {unit && <span className="text-xs font-semibold text-slate-500 dark:text-slate-400">{unit}</span>}
    </div>
    <div className="mt-0.5 text-[11px] text-slate-500 dark:text-slate-400">{sub}</div>
  </div>
);

