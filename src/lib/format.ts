export function formatBytes(bytes: number | null | undefined, decimals = 2): string {
  if (bytes === null || bytes === undefined || isNaN(bytes)) return "Unknown";
  if (bytes <= 0) return "0 B";
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
}

/**
 * Formats bits-per-second into human-readable Mbps / Gbps / Kbps.
 */
export function formatBitsPerSec(bps: number | null | undefined): { value: string; unit: string; full: string } {
  if (!bps || bps <= 0 || isNaN(bps)) {
    return { value: "0.00", unit: "Mbps", full: "0.00 Mbps" };
  }
  if (bps >= 1_000_000_000) {
    const val = (bps / 1_000_000_000).toFixed(2);
    return { value: val, unit: "Gbps", full: `${val} Gbps` };
  }
  if (bps >= 1_000_000) {
    const val = (bps / 1_000_000).toFixed(2);
    return { value: val, unit: "Mbps", full: `${val} Mbps` };
  }
  if (bps >= 1_000) {
    const val = (bps / 1_000).toFixed(1);
    return { value: val, unit: "Kbps", full: `${val} Kbps` };
  }
  const val = Math.round(bps).toString();
  return { value: val, unit: "bps", full: `${val} bps` };
}

/**
 * Formats bits-per-second into Byte transfer rate (MB/s, KB/s) used in download rows.
 */
export function formatByteSpeed(bps: number | null | undefined): string {
  if (!bps || bps <= 0 || isNaN(bps)) return "0 B/s";
  const bytesPerSec = bps / 8;
  if (bytesPerSec >= 1024 * 1024 * 1024) {
    return `${(bytesPerSec / (1024 * 1024 * 1024)).toFixed(2)} GB/s`;
  }
  if (bytesPerSec >= 1024 * 1024) {
    return `${(bytesPerSec / (1024 * 1024)).toFixed(2)} MB/s`;
  }
  if (bytesPerSec >= 1024) {
    return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
  }
  return `${Math.round(bytesPerSec)} B/s`;
}

export function formatEta(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !isFinite(seconds) || seconds < 0) {
    return "—";
  }
  if (seconds === 0) return "Done";
  const hrs = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);
  if (hrs > 0) return `${hrs}h ${mins}m ${secs}s`;
  if (mins > 0) return `${mins}m ${secs}s`;
  return `${secs}s`;
}

export function formatTimestamp(ms: number): string {
  if (!ms) return "—";
  const d = new Date(ms);
  return d.toLocaleString([], {
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function formatTimeShort(ms: number): string {
  const d = new Date(ms);
  return d.toLocaleTimeString([], { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

/**
 * Formats millisecond durations into human-readable download durations (e.g. "0.45s", "14.2s", "2m 08.4s", "1h 12m 05s").
 */
export function formatDurationMs(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !isFinite(ms) || ms <= 0) {
    return "0.0s";
  }
  if (ms < 1000) {
    return `${Math.max(0.01, ms / 1000).toFixed(2)}s`;
  }
  const totalSecs = ms / 1000;
  if (totalSecs < 60) {
    return `${totalSecs.toFixed(1)}s`;
  }
  const wholeSecs = Math.floor(totalSecs);
  if (wholeSecs < 3600) {
    const mins = Math.floor(wholeSecs / 60);
    const remSecs = ((ms % 60_000) / 1000).toFixed(1);
    return `${mins}m ${remSecs}s`;
  }
  const hrs = Math.floor(wholeSecs / 3600);
  const mins = Math.floor((wholeSecs % 3600) / 60);
  const secs = wholeSecs % 60;
  return `${hrs}h ${mins}m ${secs}s`;
}

/**
 * Returns the real active download time in milliseconds for a download task,
 * falling back to (completed_at_ms - created_at_ms) for legacy rows if needed.
 */
export function getDownloadDurationMs(item: {
  elapsed_ms?: number | null;
  created_at_ms: number;
  completed_at_ms: number | null;
}): number {
  if (item.elapsed_ms && item.elapsed_ms > 0) {
    return item.elapsed_ms;
  }
  if (item.completed_at_ms && item.created_at_ms && item.completed_at_ms > item.created_at_ms) {
    return item.completed_at_ms - item.created_at_ms;
  }
  return 0;
}

export function bondingModeDisplayName(mode: string): string {
  switch (mode) {
    case "Smart":
      return "Adaptive Bonding";
    case "MaxSpeed":
      return "Speed Priority";
    case "Balanced":
      return "Stability Priority";
    case "Failover":
      return "Failover Mode";
    case "Custom":
      return "Download Acceleration";
    default:
      return mode;
  }
}



