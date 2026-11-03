import React from "react";
import { motion } from "framer-motion";
import {
  LayoutDashboard,
  Download,
  GitMerge,
  Activity,
  Gauge,
  Settings,
  ScrollText,
  Zap,
} from "lucide-react";
import clsx from "clsx";
import { formatBitsPerSec } from "../lib/format";
import type { BondingMode, TunnelStatus } from "../lib/types";

export type NavTab =
  | "dashboard"
  | "downloads"
  | "bonding"
  | "monitor"
  | "speedtest"
  | "settings"
  | "logs";

interface SidebarProps {
  activeTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  activeDownloadsCount: number;
  totalDownloadBps: number;
  activeInterfacesCount: number;
  bondingMode: BondingMode;
  tunnelStatus: TunnelStatus;
}

const NAV_ITEMS: { id: NavTab; label: string; icon: React.ComponentType<{ className?: string }> }[] = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "downloads", label: "Downloads", icon: Download },
  { id: "bonding", label: "Network Bonding", icon: GitMerge },
  { id: "monitor", label: "Network Monitor", icon: Activity },
  { id: "speedtest", label: "Speed Test", icon: Gauge },
  { id: "settings", label: "Settings", icon: Settings },
  { id: "logs", label: "Logs", icon: ScrollText },
];

export const Sidebar: React.FC<SidebarProps> = ({
  activeTab,
  onSelectTab,
  activeDownloadsCount,
  totalDownloadBps,
  activeInterfacesCount,
  bondingMode,
  tunnelStatus,
}) => {
  const speed = formatBitsPerSec(totalDownloadBps);

  return (
    <aside className="w-64 shrink-0 h-full flex flex-col justify-between p-4 border-r border-slate-200/70 dark:border-white/[0.07] bg-white/55 dark:bg-navy-900/65 backdrop-blur-2xl select-none">
      {/* Brand Header */}
      <div>
        <div className="flex items-center gap-3 px-2 py-2 mb-6">
          <div className="relative w-10 h-10 rounded-xl bg-gradient-to-br from-electric-400 via-indigo-500 to-lavender-500 flex items-center justify-center shadow-glow-blue">
            <Zap className="w-5 h-5 text-white fill-white/20" />
            <span className="absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full bg-emerald-400 ring-2 ring-white dark:ring-navy-900" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <span className="font-bold text-lg tracking-tight bg-gradient-to-r from-slate-900 to-indigo-700 dark:from-white dark:via-electric-400 dark:to-lavender-300 bg-clip-text text-transparent">
                NetFusion
              </span>
              <span className="text-[10px] font-semibold uppercase tracking-wider px-1.5 py-0.5 rounded bg-lavender-500/15 text-lavender-500 dark:text-lavender-300 border border-lavender-400/25">
                Pro
              </span>
            </div>
            <p className="text-[11px] text-slate-500 dark:text-slate-400">
              Multipath Bonding Engine
            </p>
          </div>
        </div>

        {/* Navigation Links */}
        <nav className="space-y-1">
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const isActive = activeTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => onSelectTab(item.id)}
                className={clsx(
                  "relative w-full flex items-center justify-between px-3.5 py-2.5 rounded-xl text-sm font-medium transition-colors duration-150",
                  isActive
                    ? "text-indigo-700 dark:text-white"
                    : "text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-200 hover:bg-slate-200/50 dark:hover:bg-white/[0.04]"
                )}
              >
                {isActive && (
                  <motion.div
                    layoutId="sidebar-active-pill"
                    transition={{ type: "spring", stiffness: 420, damping: 32 }}
                    className="absolute inset-0 rounded-xl bg-gradient-to-r from-electric-500/15 via-indigo-500/15 to-lavender-500/20 border border-electric-400/30 dark:border-lavender-400/30 shadow-sm"
                  />
                )}
                <span className="relative z-10 flex items-center gap-3">
                  <Icon
                    className={clsx(
                      "w-4 h-4 transition-colors",
                      isActive
                        ? "text-electric-500 dark:text-electric-400"
                        : "text-slate-400 dark:text-slate-500"
                    )}
                  />
                  {item.label}
                </span>
                {item.id === "downloads" && activeDownloadsCount > 0 && (
                  <span className="relative z-10 px-2 py-0.5 text-[11px] font-semibold rounded-full bg-electric-500/20 text-electric-600 dark:text-electric-400 border border-electric-400/30 tabular-nums">
                    {activeDownloadsCount}
                  </span>
                )}
              </button>
            );
          })}
        </nav>
      </div>

      {/* Live Compact Telemetry Footer Card */}
      <div className="glass-card rounded-2xl p-3.5 space-y-2.5">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">
            Bonding Core
          </span>
          <span className="text-[11px] font-medium px-2 py-0.5 rounded-full bg-lavender-500/15 text-lavender-600 dark:text-lavender-300 border border-lavender-400/25">
            {bondingMode}
          </span>
        </div>

        <div className="flex items-baseline justify-between">
          <div>
            <span className="text-xl font-bold tabular-nums text-slate-900 dark:text-white">
              {speed.value}
            </span>
            <span className="ml-1 text-xs font-medium text-electric-500 dark:text-electric-400">
              {speed.unit}
            </span>
          </div>
          <span className="text-xs text-slate-500 dark:text-slate-400 tabular-nums">
            {activeInterfacesCount} {activeInterfacesCount === 1 ? "Link" : "Links"}
          </span>
        </div>

        <div className="pt-2 border-t border-slate-200/70 dark:border-white/[0.07] flex items-center justify-between text-[11px]">
          <span className="text-slate-500 dark:text-slate-400">Tunnel Server</span>
          <span
            className={clsx(
              "font-medium flex items-center gap-1.5",
              tunnelStatus === "Connected"
                ? "text-emerald-500 dark:text-emerald-400"
                : tunnelStatus === "Connecting" || tunnelStatus === "Degraded"
                ? "text-amber-500 dark:text-amber-400"
                : "text-slate-500 dark:text-slate-400"
            )}
          >
            <span
              className={clsx(
                "w-1.5 h-1.5 rounded-full",
                tunnelStatus === "Connected"
                  ? "bg-emerald-400"
                  : tunnelStatus === "Connecting" || tunnelStatus === "Degraded"
                  ? "bg-amber-400 animate-pulse"
                  : "bg-slate-400"
              )}
            />
            {tunnelStatus === "Connected"
              ? "Bonded Tunnel"
              : tunnelStatus === "ServerUnreachable"
              ? "Unavailable"
              : "Direct Multi-Path"}
          </span>
        </div>
      </div>
    </aside>
  );
};

