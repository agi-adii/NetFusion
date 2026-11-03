import React, { useState } from "react";
import {
  Pause,
  Play,
  X,
  RotateCcw,
  FolderOpen,
  ExternalLink,
  Plus,
  Trash2,
  Search,
  ShieldCheck,
  ShieldAlert,
  Clock,
  Layers,
  Gauge,
} from "lucide-react";
import clsx from "clsx";
import {
  formatBytes,
  formatByteSpeed,
  formatEta,
  formatTimestamp,
} from "../lib/format";
import type { DownloadItem, DownloadPriority, DownloadStatus } from "../lib/types";
import { FileTypeIcon, InterfaceIcon } from "../components/InterfaceBadge";

interface DownloadsPageProps {
  downloads: DownloadItem[];
  onOpenAddModal: () => void;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onRetry: (id: string) => void;
  onCancel: (id: string, deleteFiles: boolean) => void;
  onUpdatePriority: (id: string, priority: DownloadPriority) => void;
  onUpdateSpeedLimit: (id: string, speedLimitBps: number | null) => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onClearCompleted: () => void;
  onOpenFile: (id: string) => void;
  onOpenFolder: (id: string) => void;
}

type FilterTab = "all" | "active" | "scheduled" | "completed" | "failed";

export const DownloadsPage: React.FC<DownloadsPageProps> = ({
  downloads,
  onOpenAddModal,
  onPause,
  onResume,
  onRetry,
  onCancel,
  onUpdatePriority,
  onUpdateSpeedLimit,
  onPauseAll,
  onResumeAll,
  onClearCompleted,
  onOpenFile,
  onOpenFolder,
}) => {
  const [tab, setTab] = useState<FilterTab>("all");
  const [query, setQuery] = useState("");
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const filtered = downloads.filter((d) => {
    if (query.trim() && !d.file_name.toLowerCase().includes(query.toLowerCase())) {
      return false;
    }
    switch (tab) {
      case "active":
        return ["Downloading", "Probing", "Paused", "Queued", "Verifying", "Merging"].includes(
          d.status
        );
      case "scheduled":
        return d.status === "Scheduled";
      case "completed":
        return d.status === "Completed";
      case "failed":
        return d.status === "Failed" || d.status === "Cancelled";
      default:
        return true;
    }
  });

  const counts = {
    all: downloads.length,
    active: downloads.filter((d) =>
      ["Downloading", "Probing", "Paused", "Queued", "Verifying", "Merging"].includes(d.status)
    ).length,
    scheduled: downloads.filter((d) => d.status === "Scheduled").length,
    completed: downloads.filter((d) => d.status === "Completed").length,
    failed: downloads.filter((d) => d.status === "Failed" || d.status === "Cancelled").length,
  };

  return (
    <div className="space-y-4 pb-6">
      {/* Toolbar: Filter Tabs + Search + Batch Controls */}
      <div className="glass-card rounded-2xl p-3.5 flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-1.5">
          {(
            [
              ["all", "All Downloads"],
              ["active", "Active Queue"],
              ["scheduled", "Scheduled"],
              ["completed", "Completed"],
              ["failed", "Failed"],
            ] as [FilterTab, string][]
          ).map(([key, label]) => (
            <button
              key={key}
              onClick={() => setTab(key)}
              className={clsx(
                "px-3 py-1.5 rounded-xl text-xs font-semibold transition-all flex items-center gap-1.5",
                tab === key
                  ? "bg-gradient-to-r from-electric-500/20 to-lavender-500/25 text-electric-600 dark:text-white border border-electric-400/35"
                  : "text-slate-600 dark:text-slate-400 hover:bg-slate-200/60 dark:hover:bg-white/[0.05]"
              )}
            >
              <span>{label}</span>
              <span className="px-1.5 py-0.2 rounded-full text-[10px] bg-slate-200/80 dark:bg-white/10 tabular-nums">
                {counts[key]}
              </span>
            </button>
          ))}
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div className="relative">
            <Search className="w-3.5 h-3.5 absolute left-3 top-2.5 text-slate-400" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter files..."
              className="pl-8 pr-3 py-1.5 rounded-xl text-xs bg-slate-100/80 dark:bg-navy-950/70 border border-slate-300/60 dark:border-white/10 text-slate-900 dark:text-white focus:outline-none focus:border-electric-400"
            />
          </div>

          <button
            onClick={onResumeAll}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium glass-card hover:border-emerald-400/40 text-emerald-600 dark:text-emerald-300"
          >
            <Play className="w-3.5 h-3.5" />
            Resume All
          </button>
          <button
            onClick={onPauseAll}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium glass-card hover:border-amber-400/40 text-amber-600 dark:text-amber-300"
          >
            <Pause className="w-3.5 h-3.5" />
            Pause All
          </button>
          <button
            onClick={onClearCompleted}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium glass-card hover:border-rose-400/40 text-slate-600 dark:text-slate-300"
          >
            <Trash2 className="w-3.5 h-3.5" />
            Clear Completed
          </button>
        </div>
      </div>

      {/* Download List */}
      {filtered.length === 0 ? (
        <div className="glass-card rounded-2xl p-12 text-center space-y-3">
          <p className="text-sm font-semibold text-slate-700 dark:text-slate-200">
            No downloads in this view
          </p>
          <p className="text-xs text-slate-500 dark:text-slate-400 max-w-md mx-auto">
            Add any HTTP/HTTPS file link to automatically split it into parallel byte-range segments distributed across Wi-Fi, Ethernet, USB tethering, or your Bonding Server tunnel.
          </p>
          <button
            onClick={onOpenAddModal}
            className="inline-flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-semibold text-white bg-gradient-to-r from-electric-500 via-indigo-500 to-lavender-500 shadow-glow-blue"
          >
            <Plus className="w-4 h-4" />
            Add First Download
          </button>
        </div>
      ) : (
        <div className="space-y-3">
          {filtered.map((item) => {
            const pct =
              item.total_bytes && item.total_bytes > 0
                ? Math.min(100, (item.downloaded_bytes / item.total_bytes) * 100)
                : item.status === "Completed"
                ? 100
                : 0;
            const isExpanded = expandedId === item.id;

            return (
              <div
                key={item.id}
                className="glass-card rounded-2xl p-4 transition-all hover:border-lavender-400/35"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  {/* Left: File Icon + Metadata */}
                  <div className="flex items-start gap-3.5 min-w-0 flex-1">
                    <FileTypeIcon fileName={item.file_name} className="w-5 h-5" />
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <h3 className="text-sm font-bold text-slate-900 dark:text-white truncate max-w-xl">
                          {item.file_name}
                        </h3>
                        <DownloadStatusBadge status={item.status} />
                        {item.used_bonding_tunnel && (
                          <span className="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-lavender-500/15 text-lavender-500 dark:text-lavender-300 border border-lavender-400/30">
                            Bonded Tunnel
                          </span>
                        )}
                        {item.checksum_verified === true && (
                          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-500/15 text-emerald-400 border border-emerald-400/30">
                            <ShieldCheck className="w-3 h-3" />
                            {item.checksum_algorithm} Verified
                          </span>
                        )}
                        {item.checksum_verified === false && (
                          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-rose-500/15 text-rose-400 border border-rose-400/30">
                            <ShieldAlert className="w-3 h-3" />
                            Checksum Mismatch
                          </span>
                        )}
                      </div>

                      <div className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-slate-500 dark:text-slate-400 tabular-nums">
                        <span>
                          <strong>{formatBytes(item.downloaded_bytes)}</strong> /{" "}
                          {formatBytes(item.total_bytes)} ({pct.toFixed(1)}%)
                        </span>
                        <span className="text-electric-600 dark:text-electric-400 font-semibold">
                          Now: {formatByteSpeed(item.speed_bps)}
                        </span>
                        <span>Avg: {formatByteSpeed(item.avg_speed_bps)}</span>
                        <span>ETA: {formatEta(item.eta_seconds)}</span>
                        {item.scheduled_at_ms && item.status === "Scheduled" && (
                          <span className="inline-flex items-center gap-1 text-amber-400">
                            <Clock className="w-3 h-3" />
                            Starts {formatTimestamp(item.scheduled_at_ms)}
                          </span>
                        )}
                      </div>
                    </div>
                  </div>

                  {/* Right: Action Buttons & Priority */}
                  <div className="flex items-center gap-1.5 shrink-0">
                    <select
                      value={item.priority}
                      onChange={(e) =>
                        onUpdatePriority(item.id, e.target.value as DownloadPriority)
                      }
                      title="Download Priority"
                      className="px-2 py-1 rounded-lg text-[11px] font-medium bg-slate-100 dark:bg-navy-900 border border-slate-300/60 dark:border-white/10 text-slate-700 dark:text-slate-200"
                    >
                      <option value="Low">Low</option>
                      <option value="Normal">Normal</option>
                      <option value="High">High</option>
                      <option value="Critical">Critical</option>
                    </select>

                    {item.status === "Downloading" ||
                    item.status === "Probing" ||
                    item.status === "Queued" ? (
                      <button
                        onClick={() => onPause(item.id)}
                        className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-xl text-xs font-semibold bg-amber-500/15 text-amber-500 dark:text-amber-300 border border-amber-400/30 hover:bg-amber-500/25"
                        title="Pause / Stop Download (Saves progress to resume later)"
                      >
                        <Pause className="w-3.5 h-3.5" />
                        Pause
                      </button>
                    ) : null}

                    {(item.status === "Paused" || item.status === "Scheduled") && (
                      <button
                        onClick={() => onResume(item.id)}
                        className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-xl text-xs font-semibold bg-emerald-500/15 text-emerald-500 dark:text-emerald-300 border border-emerald-400/30 hover:bg-emerald-500/25"
                        title="Resume Download from Exact Paused Byte Offset"
                      >
                        <Play className="w-3.5 h-3.5" />
                        Resume
                      </button>
                    )}

                    {(item.status === "Failed" || item.status === "Cancelled") && (
                      <button
                        onClick={() => onRetry(item.id)}
                        className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-xl text-xs font-semibold bg-electric-500/15 text-electric-500 dark:text-electric-300 border border-electric-400/30 hover:bg-electric-500/25"
                        title="Retry Download"
                      >
                        <RotateCcw className="w-3.5 h-3.5" />
                        Retry
                      </button>
                    )}

                    {item.status === "Completed" && (
                      <>
                        <button
                          onClick={() => onOpenFile(item.id)}
                          className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-xl text-xs font-semibold bg-electric-500/15 text-electric-500 dark:text-electric-300 border border-electric-400/30 hover:bg-electric-500/25"
                          title="Open File"
                        >
                          <ExternalLink className="w-3.5 h-3.5" />
                          Open
                        </button>
                        <button
                          onClick={() => onOpenFolder(item.id)}
                          className="p-1.5 rounded-xl glass-card text-slate-600 dark:text-slate-300 hover:text-electric-400"
                          title="Open Folder"
                        >
                          <FolderOpen className="w-4 h-4" />
                        </button>
                      </>
                    )}

                    {item.status !== "Completed" && (
                      <button
                        onClick={() => onOpenFolder(item.id)}
                        className="p-1.5 rounded-xl glass-card text-slate-600 dark:text-slate-300 hover:text-electric-400"
                        title="Open Destination Folder"
                      >
                        <FolderOpen className="w-4 h-4" />
                      </button>
                    )}

                    <button
                      onClick={() => setExpandedId(isExpanded ? null : item.id)}
                      className={clsx(
                        "p-1.5 rounded-xl glass-card transition-colors",
                        isExpanded
                          ? "text-electric-400 border-electric-400/40"
                          : "text-slate-600 dark:text-slate-300"
                      )}
                      title="Inspect Segments & Speed Limit"
                    >
                      <Layers className="w-4 h-4" />
                    </button>

                    <button
                      onClick={() => onCancel(item.id, item.status !== "Completed")}
                      className="p-1.5 rounded-xl glass-card text-rose-400 hover:bg-rose-500/15"
                      title={item.status === "Completed" ? "Remove from List" : "Cancel Download"}
                    >
                      <X className="w-4 h-4" />
                    </button>
                  </div>
                </div>

                {/* Main Progress Bar */}
                <div className="mt-3 h-2.5 rounded-full bg-slate-200/90 dark:bg-navy-950 overflow-hidden">
                  <div
                    className={clsx(
                      "h-full rounded-full transition-all duration-300",
                      item.status === "Completed"
                        ? "bg-gradient-to-r from-emerald-400 to-teal-400"
                        : item.status === "Failed"
                        ? "bg-rose-500"
                        : item.status === "Paused"
                        ? "bg-amber-400"
                        : "bg-gradient-to-r from-electric-400 via-indigo-400 to-lavender-400"
                    )}
                    style={{ width: `${pct.toFixed(1)}%` }}
                  />
                </div>

                {/* Active Network Paths Row */}
                <div className="mt-2.5 flex flex-wrap items-center justify-between gap-2">
                  <div className="flex flex-wrap items-center gap-1.5">
                    {item.active_paths.length > 0 ? (
                      item.active_paths.map((path) => (
                        <span
                          key={path.interface_id}
                          className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-lg text-[11px] font-medium bg-electric-500/10 text-electric-600 dark:text-electric-300 border border-electric-400/25 tabular-nums"
                        >
                          <InterfaceIcon kind={path.kind} className="w-3 h-3" />
                          <span>{path.interface_name}</span>
                          <strong>{formatByteSpeed(path.speed_bps)}</strong>
                          <span className="text-[10px] opacity-75">
                            ({formatBytes(path.bytes_contributed)})
                          </span>
                        </span>
                      ))
                    ) : (
                      <span className="text-[11px] text-slate-500 dark:text-slate-400 truncate max-w-xl">
                        {item.error_message ? (
                          <span className="text-rose-400">{item.error_message}</span>
                        ) : item.status === "Paused" && item.downloaded_bytes > 0 ? (
                          <span className="text-amber-500 dark:text-amber-300 font-medium">
                            Paused at {formatBytes(item.downloaded_bytes)} · Ready to resume from exact offset
                          </span>
                        ) : (
                          `Destination: ${item.target_path}`
                        )}
                      </span>
                    )}
                  </div>

                  {item.segments.length > 0 && (
                    <span className="text-[11px] text-slate-500 dark:text-slate-400 tabular-nums">
                      {item.segments.filter((s) => s.status === "Completed").length} /{" "}
                      {item.segments.length} Segments Complete
                    </span>
                  )}
                </div>

                {/* Expanded Per-Segment Visual Map & Task Speed Limit */}
                {isExpanded && (
                  <div className="mt-3.5 pt-3.5 border-t border-slate-200/70 dark:border-white/10 space-y-3">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <span className="text-xs font-semibold text-slate-700 dark:text-slate-300">
                        Parallel Segment Map ({item.segments.length} Segments)
                      </span>
                      <TaskSpeedLimitEditor
                        currentLimitBps={item.speed_limit_bps}
                        onApply={(bps) => onUpdateSpeedLimit(item.id, bps)}
                      />
                    </div>

                    {item.segments.length > 0 && (
                      <div className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8 gap-2">
                        {item.segments.map((seg) => {
                          const segTotal = Math.max(1, seg.end_byte - seg.start_byte + 1);
                          const segPct = Math.min(100, (seg.downloaded_bytes / segTotal) * 100);
                          return (
                            <div
                              key={seg.index}
                              className="p-2 rounded-xl bg-slate-100/80 dark:bg-navy-950/75 border border-slate-200/70 dark:border-white/[0.07] text-[10px]"
                            >
                              <div className="flex items-center justify-between font-semibold text-slate-700 dark:text-slate-300">
                                <span>#{seg.index + 1}</span>
                                <span>{Math.round(segPct)}%</span>
                              </div>
                              <div className="my-1 h-1.5 rounded-full bg-slate-300/70 dark:bg-white/10 overflow-hidden">
                                <div
                                  className={clsx(
                                    "h-full rounded-full",
                                    seg.status === "Completed"
                                      ? "bg-emerald-400"
                                      : seg.status === "Active"
                                      ? "bg-electric-400"
                                      : seg.status === "Failed"
                                      ? "bg-rose-400"
                                      : "bg-slate-400"
                                  )}
                                  style={{ width: `${segPct}%` }}
                                />
                              </div>
                              <div className="truncate text-slate-500 dark:text-slate-400">
                                {seg.assigned_interface_name || seg.status}
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    )}

                    {item.computed_checksum && (
                      <div className="text-[11px] font-mono text-slate-500 dark:text-slate-400 break-all">
                        Computed Digest: {item.computed_checksum}
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};

const DownloadStatusBadge: React.FC<{ status: DownloadStatus }> = ({ status }) => {
  const map: Record<DownloadStatus, string> = {
    Downloading: "bg-electric-500/15 text-electric-500 dark:text-electric-300 border-electric-400/30",
    Probing: "bg-indigo-500/15 text-indigo-400 border-indigo-400/30",
    Queued: "bg-slate-500/15 text-slate-400 border-slate-400/30",
    Scheduled: "bg-amber-500/15 text-amber-400 border-amber-400/30",
    Paused: "bg-amber-500/15 text-amber-400 border-amber-400/30",
    Verifying: "bg-lavender-500/15 text-lavender-300 border-lavender-400/30",
    Merging: "bg-lavender-500/15 text-lavender-300 border-lavender-400/30",
    Completed: "bg-emerald-500/15 text-emerald-400 border-emerald-400/30",
    Failed: "bg-rose-500/15 text-rose-400 border-rose-400/30",
    Cancelled: "bg-slate-500/15 text-slate-400 border-slate-400/30",
  };
  return (
    <span className={clsx("px-2 py-0.5 rounded-full text-[10px] font-semibold border", map[status])}>
      {status}
    </span>
  );
};

const TaskSpeedLimitEditor: React.FC<{
  currentLimitBps: number | null;
  onApply: (bps: number | null) => void;
}> = ({ currentLimitBps, onApply }) => {
  const initialMb = currentLimitBps ? (currentLimitBps / (8 * 1024 * 1024)).toFixed(1) : "";
  const [val, setVal] = useState(initialMb);

  return (
    <div className="flex items-center gap-1.5 text-xs">
      <Gauge className="w-3.5 h-3.5 text-lavender-400" />
      <span className="text-slate-500 dark:text-slate-400">Limit (MB/s):</span>
      <input
        type="number"
        step="0.5"
        min="0"
        value={val}
        onChange={(e) => setVal(e.target.value)}
        placeholder="None"
        className="w-20 px-2 py-1 rounded-lg text-xs bg-white dark:bg-navy-900 border border-slate-300/70 dark:border-white/10"
      />
      <button
        onClick={() => {
          const parsed = parseFloat(val);
          if (!val.trim() || isNaN(parsed) || parsed <= 0) {
            onApply(null);
          } else {
            onApply(Math.round(parsed * 8 * 1024 * 1024));
          }
        }}
        className="px-2.5 py-1 rounded-lg text-[11px] font-semibold bg-lavender-500/20 text-lavender-600 dark:text-lavender-300 border border-lavender-400/30 hover:bg-lavender-500/30"
      >
        Apply
      </button>
    </div>
  );
};

