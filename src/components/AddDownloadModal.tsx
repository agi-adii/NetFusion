import React, { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  X,
  FolderOpen,
  Sparkles,
  ShieldCheck,
  Clock,
  Gauge,
  AlertCircle,
  Loader2,
  CheckCircle2,
  Link2,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, isTauriRuntime } from "../lib/tauri";
import { formatBytes } from "../lib/format";
import type { AddDownloadRequest, DownloadPriority, UrlProbeResult } from "../lib/types";

interface AddDownloadModalProps {
  isOpen: boolean;
  defaultSaveDir: string;
  defaultSegments: number;
  onClose: () => void;
  onAdded: () => void;
}

export const AddDownloadModal: React.FC<AddDownloadModalProps> = ({
  isOpen,
  defaultSaveDir,
  defaultSegments,
  onClose,
  onAdded,
}) => {
  const [url, setUrl] = useState("");
  const [fileName, setFileName] = useState("");
  const [saveDir, setSaveDir] = useState(defaultSaveDir);
  const [priority, setPriority] = useState<DownloadPriority>("Normal");
  const [maxSegments, setMaxSegments] = useState(defaultSegments || 8);
  const [speedLimitMBps, setSpeedLimitMBps] = useState<string>("");
  const [scheduleEnabled, setScheduleEnabled] = useState(false);
  const [scheduledDateTime, setScheduledDateTime] = useState("");
  const [checksumAlgo, setChecksumAlgo] = useState<"NONE" | "SHA256" | "MD5">("NONE");
  const [expectedChecksum, setExpectedChecksum] = useState("");
  const [probe, setProbe] = useState<UrlProbeResult | null>(null);
  const [probing, setProbing] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (defaultSaveDir && !saveDir) {
      setSaveDir(defaultSaveDir);
    }
  }, [defaultSaveDir]);

  useEffect(() => {
    if (!isOpen) {
      setUrl("");
      setFileName("");
      setProbe(null);
      setError(null);
      setScheduleEnabled(false);
      setScheduledDateTime("");
      setChecksumAlgo("NONE");
      setExpectedChecksum("");
      setSpeedLimitMBps("");
    }
  }, [isOpen]);

  // Debounced URL probe for size, Accept-Ranges, and suggested filename
  useEffect(() => {
    const trimmed = url.trim();
    if (!trimmed || (!trimmed.startsWith("http://") && !trimmed.startsWith("https://"))) {
      setProbe(null);
      return;
    }
    const timer = setTimeout(async () => {
      if (!isTauriRuntime()) return;
      setProbing(true);
      setError(null);
      try {
        const res = await api.probeDownloadUrl(trimmed);
        setProbe(res);
        if (res.suggested_file_name && !fileName) {
          setFileName(res.suggested_file_name);
        }
      } catch (err: any) {
        setProbe(null);
        setError(typeof err === "string" ? err : err?.message || "Failed to inspect URL");
      } finally {
        setProbing(false);
      }
    }, 450);

    return () => clearTimeout(timer);
  }, [url]);

  const handleBrowseFolder = async () => {
    if (!isTauriRuntime()) return;
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: saveDir || undefined,
      title: "Select Download Destination Folder",
    });
    if (typeof selected === "string") {
      setSaveDir(selected);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    const trimmed = url.trim();
    if (!trimmed.startsWith("http://") && !trimmed.startsWith("https://")) {
      setError("Please enter a valid HTTP or HTTPS download URL.");
      return;
    }

    let scheduledAtMs: number | null = null;
    if (scheduleEnabled && scheduledDateTime) {
      const parsed = new Date(scheduledDateTime).getTime();
      if (isNaN(parsed) || parsed <= Date.now()) {
        setError("Scheduled start time must be in the future.");
        return;
      }
      scheduledAtMs = parsed;
    }

    let speedLimitBps: number | null = null;
    if (speedLimitMBps.trim() !== "") {
      const mb = parseFloat(speedLimitMBps);
      if (isNaN(mb) || mb <= 0) {
        setError("Speed limit must be a positive number in MB/s.");
        return;
      }
      speedLimitBps = Math.round(mb * 1024 * 1024 * 8);
    }

    const request: AddDownloadRequest = {
      url: trimmed,
      file_name: fileName.trim() || undefined,
      save_dir: saveDir.trim() || undefined,
      priority,
      max_segments: maxSegments,
      speed_limit_bps: speedLimitBps,
      scheduled_at_ms: scheduledAtMs,
      expected_checksum:
        checksumAlgo !== "NONE" && expectedChecksum.trim()
          ? expectedChecksum.trim()
          : null,
      checksum_algorithm: checksumAlgo === "NONE" ? null : checksumAlgo,
      start_immediately: !scheduledAtMs,
    };

    setSubmitting(true);
    try {
      await api.addDownload(request);
      onAdded();
      onClose();
    } catch (err: any) {
      setError(typeof err === "string" ? err : err?.message || "Failed to add download.");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/70 backdrop-blur-md">
          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ duration: 0.18 }}
            className="w-full max-w-2xl glass-card-elevated rounded-2xl p-6 shadow-2xl border border-lavender-400/25 max-h-[90vh] overflow-y-auto"
          >
            <div className="flex items-center justify-between pb-4 border-b border-slate-200/70 dark:border-white/10">
              <div className="flex items-center gap-3">
                <div className="p-2.5 rounded-xl bg-gradient-to-br from-electric-500/20 to-lavender-500/20 text-electric-400 border border-electric-400/30">
                  <Sparkles className="w-5 h-5" />
                </div>
                <div>
                  <h2 className="text-base font-bold text-slate-900 dark:text-white">
                    New Accelerated Download
                  </h2>
                  <p className="text-xs text-slate-500 dark:text-slate-400">
                    Segments are dynamically distributed across all active bonded networks
                  </p>
                </div>
              </div>
              <button
                onClick={onClose}
                className="p-1.5 rounded-lg text-slate-400 hover:text-slate-700 dark:hover:text-white hover:bg-white/10 transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            <form onSubmit={handleSubmit} className="mt-5 space-y-4">
              {/* URL Input */}
              <div>
                <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
                  Download URL (HTTP / HTTPS)
                </label>
                <div className="relative">
                  <Link2 className="w-4 h-4 absolute left-3.5 top-3 text-slate-400" />
                  <input
                    type="url"
                    required
                    autoFocus
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                    placeholder="https://releases.ubuntu.com/... or any direct file link"
                    className="w-full pl-10 pr-10 py-2.5 rounded-xl text-sm bg-slate-100/80 dark:bg-navy-950/70 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white focus:outline-none focus:border-electric-400"
                  />
                  {probing && (
                    <Loader2 className="w-4 h-4 absolute right-3.5 top-3 text-electric-400 animate-spin" />
                  )}
                </div>

                {/* Live Probe Metadata Banner */}
                {probe && (
                  <div className="mt-2 p-2.5 rounded-xl bg-electric-500/10 border border-electric-400/25 flex flex-wrap items-center justify-between gap-2 text-xs">
                    <div className="flex items-center gap-2 text-slate-700 dark:text-slate-200">
                      <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                      <span>
                        Size: <strong>{formatBytes(probe.total_bytes)}</strong>
                      </span>
                      {probe.content_type && (
                        <span className="text-slate-500 dark:text-slate-400">
                          · {probe.content_type.split(";")[0]}
                        </span>
                      )}
                    </div>
                    <span
                      className={
                        probe.supports_range
                          ? "px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-500 dark:text-emerald-300 font-medium"
                          : "px-2 py-0.5 rounded-full bg-amber-500/15 text-amber-500 dark:text-amber-300 font-medium"
                      }
                    >
                      {probe.supports_range
                        ? "Multi-Segment Acceleration Supported"
                        : "Single-Stream Server (Use Bonding Tunnel for Multi-NIC)"}
                    </span>
                  </div>
                )}
              </div>

              {/* File Name & Destination */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3.5">
                <div>
                  <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
                    File Name (Auto-detected if blank)
                  </label>
                  <input
                    type="text"
                    value={fileName}
                    onChange={(e) => setFileName(e.target.value)}
                    placeholder="Auto-named from URL / headers"
                    className="w-full px-3.5 py-2 rounded-xl text-sm bg-slate-100/80 dark:bg-navy-950/70 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white focus:outline-none focus:border-electric-400"
                  />
                </div>

                <div>
                  <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
                    Save Location
                  </label>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={saveDir}
                      onChange={(e) => setSaveDir(e.target.value)}
                      className="w-full px-3 py-2 rounded-xl text-sm bg-slate-100/80 dark:bg-navy-950/70 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white focus:outline-none focus:border-electric-400"
                    />
                    <button
                      type="button"
                      onClick={handleBrowseFolder}
                      className="px-3 py-2 rounded-xl glass-card hover:border-electric-400/40 text-slate-600 dark:text-slate-300"
                      title="Browse Folder"
                    >
                      <FolderOpen className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              </div>

              {/* Priority, Segments, Speed Limit */}
              <div className="grid grid-cols-1 md:grid-cols-3 gap-3.5">
                <div>
                  <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
                    Queue Priority
                  </label>
                  <select
                    value={priority}
                    onChange={(e) => setPriority(e.target.value as DownloadPriority)}
                    className="w-full px-3 py-2 rounded-xl text-sm bg-slate-100/80 dark:bg-navy-950/70 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white focus:outline-none focus:border-electric-400"
                  >
                    <option value="Low">Low</option>
                    <option value="Normal">Normal</option>
                    <option value="High">High</option>
                    <option value="Critical">Critical (Top Queue)</option>
                  </select>
                </div>

                <div>
                  <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
                    Parallel Segments ({maxSegments})
                  </label>
                  <input
                    type="range"
                    min={1}
                    max={32}
                    value={maxSegments}
                    onChange={(e) => setMaxSegments(Number(e.target.value))}
                    className="w-full accent-electric-500 mt-2"
                  />
                </div>

                <div>
                  <label className="flex items-center gap-1.5 text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
                    <Gauge className="w-3.5 h-3.5 text-lavender-400" />
                    Speed Limit (MB/s)
                  </label>
                  <input
                    type="number"
                    step="0.1"
                    min="0"
                    value={speedLimitMBps}
                    onChange={(e) => setSpeedLimitMBps(e.target.value)}
                    placeholder="Unlimited"
                    className="w-full px-3 py-2 rounded-xl text-sm bg-slate-100/80 dark:bg-navy-950/70 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white focus:outline-none focus:border-electric-400"
                  />
                </div>
              </div>

              {/* Checksum Verification & Scheduling */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3.5 pt-1">
                <div className="p-3 rounded-xl bg-slate-100/60 dark:bg-white/[0.03] border border-slate-200/70 dark:border-white/[0.07] space-y-2">
                  <div className="flex items-center justify-between">
                    <span className="flex items-center gap-1.5 text-xs font-semibold text-slate-700 dark:text-slate-300">
                      <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
                      Integrity Checksum
                    </span>
                    <select
                      value={checksumAlgo}
                      onChange={(e) => setChecksumAlgo(e.target.value as any)}
                      className="text-xs px-2 py-1 rounded-lg bg-white dark:bg-navy-900 border border-slate-300/70 dark:border-white/10 text-slate-800 dark:text-slate-200"
                    >
                      <option value="NONE">Optional</option>
                      <option value="SHA256">SHA-256</option>
                      <option value="MD5">MD5</option>
                    </select>
                  </div>
                  {checksumAlgo !== "NONE" && (
                    <input
                      type="text"
                      value={expectedChecksum}
                      onChange={(e) => setExpectedChecksum(e.target.value)}
                      placeholder={`Paste expected ${checksumAlgo} hex digest`}
                      className="w-full px-2.5 py-1.5 rounded-lg text-xs font-mono bg-white dark:bg-navy-950 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white"
                    />
                  )}
                </div>

                <div className="p-3 rounded-xl bg-slate-100/60 dark:bg-white/[0.03] border border-slate-200/70 dark:border-white/[0.07] space-y-2">
                  <label className="flex items-center justify-between cursor-pointer">
                    <span className="flex items-center gap-1.5 text-xs font-semibold text-slate-700 dark:text-slate-300">
                      <Clock className="w-3.5 h-3.5 text-electric-400" />
                      Schedule Download
                    </span>
                    <input
                      type="checkbox"
                      checked={scheduleEnabled}
                      onChange={(e) => setScheduleEnabled(e.target.checked)}
                      className="rounded accent-electric-500"
                    />
                  </label>
                  {scheduleEnabled && (
                    <input
                      type="datetime-local"
                      value={scheduledDateTime}
                      onChange={(e) => setScheduledDateTime(e.target.value)}
                      className="w-full px-2.5 py-1.5 rounded-lg text-xs bg-white dark:bg-navy-950 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white"
                    />
                  )}
                </div>
              </div>

              {error && (
                <div className="p-3 rounded-xl bg-rose-500/15 border border-rose-500/30 flex items-center gap-2 text-xs text-rose-500 dark:text-rose-300">
                  <AlertCircle className="w-4 h-4 shrink-0" />
                  <span>{error}</span>
                </div>
              )}

              <div className="flex items-center justify-end gap-2.5 pt-3 border-t border-slate-200/70 dark:border-white/10">
                <button
                  type="button"
                  onClick={onClose}
                  className="px-4 py-2 rounded-xl text-xs font-semibold text-slate-600 dark:text-slate-300 hover:bg-slate-200/60 dark:hover:bg-white/10 transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={submitting}
                  className="inline-flex items-center gap-2 px-5 py-2 rounded-xl text-xs font-semibold text-white bg-gradient-to-r from-electric-500 via-indigo-500 to-lavender-500 hover:brightness-110 disabled:opacity-50 shadow-glow-blue"
                >
                  {submitting && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                  {scheduleEnabled ? "Schedule Download" : "Start Bonded Download"}
                </button>
              </div>
            </form>
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
};

