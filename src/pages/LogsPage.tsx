import React, { useState } from "react";
import { Trash2, Copy, Search, Check } from "lucide-react";
import clsx from "clsx";
import { formatTimestamp } from "../lib/format";
import type { LogEntry } from "../lib/types";

interface LogsPageProps {
  logs: LogEntry[];
  onClear: () => Promise<void>;
}

export const LogsPage: React.FC<LogsPageProps> = ({ logs, onClear }) => {
  const [category, setCategory] = useState<string>("ALL");
  const [search, setSearch] = useState("");
  const [copied, setCopied] = useState(false);

  const filtered = logs.filter((l) => {
    if (category !== "ALL" && l.category !== category) return false;
    if (search.trim() && !l.message.toLowerCase().includes(search.toLowerCase())) {
      return false;
    }
    return true;
  });

  const handleCopy = async () => {
    const text = filtered
      .map(
        (l) =>
          `[${new Date(l.timestamp_ms).toISOString()}] [${l.level}] [${l.category}] ${l.message}`
      )
      .join("\n");
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="space-y-4 pb-6">
      <div className="glass-card rounded-2xl p-3.5 flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-1.5">
          {["ALL", "BONDING", "NETWORK", "DOWNLOAD", "SPEEDTEST", "SYSTEM"].map((cat) => (
            <button
              key={cat}
              onClick={() => setCategory(cat)}
              className={clsx(
                "px-3 py-1.5 rounded-xl text-xs font-semibold transition-colors",
                category === cat
                  ? "bg-gradient-to-r from-electric-500/20 to-lavender-500/20 text-electric-600 dark:text-white border border-electric-400/35"
                  : "text-slate-600 dark:text-slate-400 hover:bg-slate-200/60 dark:hover:bg-white/[0.05]"
              )}
            >
              {cat}
            </button>
          ))}
        </div>

        <div className="flex items-center gap-2">
          <div className="relative">
            <Search className="w-3.5 h-3.5 absolute left-3 top-2.5 text-slate-400" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search event logs..."
              className="pl-8 pr-3 py-1.5 rounded-xl text-xs bg-slate-100 dark:bg-navy-950 border border-slate-300/60 dark:border-white/10"
            />
          </div>

          <button
            onClick={handleCopy}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium glass-card text-slate-700 dark:text-slate-200"
          >
            {copied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
            {copied ? "Copied" : "Copy"}
          </button>

          <button
            onClick={onClear}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium glass-card text-rose-400"
          >
            <Trash2 className="w-3.5 h-3.5" />
            Clear
          </button>
        </div>
      </div>

      <div className="glass-card rounded-2xl p-4 font-mono text-xs max-h-[calc(100vh-220px)] overflow-y-auto divide-y divide-slate-200/50 dark:divide-white/[0.05]">
        {filtered.length === 0 ? (
          <div className="py-12 text-center font-sans text-slate-500 dark:text-slate-400">
            No matching log events recorded.
          </div>
        ) : (
          filtered.map((entry) => (
            <div key={entry.id} className="py-2 flex items-start gap-3">
              <span className="text-[11px] text-slate-400 shrink-0 tabular-nums">
                {formatTimestamp(entry.timestamp_ms)}
              </span>
              <span
                className={clsx(
                  "px-1.5 py-0.2 rounded text-[10px] font-bold shrink-0",
                  entry.level === "ERROR"
                    ? "bg-rose-500/20 text-rose-400"
                    : entry.level === "WARN"
                    ? "bg-amber-500/20 text-amber-400"
                    : entry.level === "SUCCESS"
                    ? "bg-emerald-500/20 text-emerald-400"
                    : "bg-electric-500/15 text-electric-400"
                )}
              >
                {entry.level}
              </span>
              <span className="px-1.5 py-0.2 rounded text-[10px] bg-lavender-500/15 text-lavender-400 shrink-0">
                {entry.category}
              </span>
              <span className="text-slate-800 dark:text-slate-200 break-all">
                {entry.message}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
};

