import React from "react";
import {
  Plus,
  Settings,
  Sun,
  Moon,
  ShieldCheck,
  AlertTriangle,
  WifiOff,
  Server,
} from "lucide-react";
import clsx from "clsx";
import type { BondingStateSnapshot } from "../lib/types";
import type { NavTab } from "./Sidebar";

interface TopBarProps {
  activeTab: NavTab;
  activeConnectionsCount: number;
  bonding: BondingStateSnapshot;
  theme: "dark" | "light";
  onToggleTheme: () => void;
  onOpenAddDownload: () => void;
  onNavigate: (tab: NavTab) => void;
}

const PAGE_TITLES: Record<NavTab, { title: string; subtitle: string }> = {
  dashboard: {
    title: "Command Dashboard",
    subtitle: "Real-time aggregate multi-network bandwidth & active download acceleration",
  },
  downloads: {
    title: "Download Manager",
    subtitle: "Multi-connection segmented downloads distributed across bonded interfaces",
  },
  bonding: {
    title: "Network Bonding Engine",
    subtitle: "Configure multipath traffic scheduling, adapter weights, and encrypted VPS bonding tunnel",
  },
  monitor: {
    title: "Network Monitor",
    subtitle: "Live Windows IP Helper interface telemetry, RTT ping, packet loss & stability diagnostics",
  },
  speedtest: {
    title: "Internet Speed Test",
    subtitle: "Measure real aggregate bonded throughput or benchmark individual network interfaces",
  },
  settings: {
    title: "Application Settings",
    subtitle: "Download preferences, bonding server credentials, Windows startup & system tray controls",
  },
  logs: {
    title: "System & Bonding Logs",
    subtitle: "Real-time diagnostic audit trail for multipath subflows, failover events, and downloads",
  },
};

export const TopBar: React.FC<TopBarProps> = ({
  activeTab,
  activeConnectionsCount,
  bonding,
  theme,
  onToggleTheme,
  onOpenAddDownload,
  onNavigate,
}) => {
  const header = PAGE_TITLES[activeTab];

  const isOnline = activeConnectionsCount > 0;
  const tunnelUp = bonding.tunnel_status === "Connected";
  const serverDown =
    bonding.tunnel_enabled &&
    (bonding.tunnel_status === "ServerUnreachable" || bonding.tunnel_status === "AuthFailed");

  return (
    <header className="h-18 shrink-0 px-6 py-3.5 border-b border-slate-200/70 dark:border-white/[0.07] bg-white/40 dark:bg-navy-900/45 backdrop-blur-xl flex items-center justify-between gap-4">
      <div className="min-w-0">
        <h1 className="text-lg font-bold tracking-tight text-slate-900 dark:text-white truncate">
          {header.title}
        </h1>
        <p className="text-xs text-slate-500 dark:text-slate-400 truncate">{header.subtitle}</p>
      </div>

      <div className="flex items-center gap-2.5 shrink-0">
        {/* Global Network + Bonding Tunnel Status Pill */}
        <button
          onClick={() => onNavigate("bonding")}
          title={bonding.status_message}
          className={clsx(
            "flex items-center gap-2 px-3 py-1.5 rounded-xl text-xs font-medium border transition-all",
            !isOnline
              ? "bg-rose-500/15 text-rose-500 dark:text-rose-400 border-rose-500/30"
              : serverDown
              ? "bg-amber-500/15 text-amber-600 dark:text-amber-300 border-amber-500/30"
              : tunnelUp
              ? "bg-emerald-500/15 text-emerald-600 dark:text-emerald-300 border-emerald-500/30"
              : "bg-electric-500/12 text-electric-600 dark:text-electric-300 border-electric-400/25"
          )}
        >
          {!isOnline ? (
            <>
              <WifiOff className="w-3.5 h-3.5" />
              <span>Offline (0 Active NICs)</span>
            </>
          ) : serverDown ? (
            <>
              <AlertTriangle className="w-3.5 h-3.5" />
              <span>
                Bonding Server Unavailable · Direct Multi-Path ({activeConnectionsCount} NICs)
              </span>
            </>
          ) : tunnelUp ? (
            <>
              <ShieldCheck className="w-3.5 h-3.5" />
              <span>
                Encrypted Tunnel Active ({bonding.active_subflows} Subflows)
              </span>
            </>
          ) : (
            <>
              <Server className="w-3.5 h-3.5" />
              <span>
                {activeConnectionsCount} {activeConnectionsCount === 1 ? "Network" : "Networks"}{" "}
                Active · Direct Multi-Path
              </span>
            </>
          )}
        </button>

        {/* New Download Primary CTA */}
        <button
          onClick={onOpenAddDownload}
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded-xl text-xs font-semibold text-white bg-gradient-to-r from-electric-500 via-indigo-500 to-lavender-500 hover:brightness-110 active:scale-[0.98] transition-all shadow-glow-blue"
        >
          <Plus className="w-4 h-4" />
          New Download
        </button>

        {/* Theme Toggle */}
        <button
          onClick={onToggleTheme}
          title={theme === "dark" ? "Switch to Light Mode" : "Switch to Dark Mode"}
          className="p-2 rounded-xl glass-card text-slate-600 dark:text-slate-300 hover:text-electric-500 dark:hover:text-electric-400 transition-colors"
        >
          {theme === "dark" ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />}
        </button>

        {/* Settings Icon */}
        <button
          onClick={() => onNavigate("settings")}
          title="Settings"
          className="p-2 rounded-xl glass-card text-slate-600 dark:text-slate-300 hover:text-lavender-500 dark:hover:text-lavender-400 transition-colors"
        >
          <Settings className="w-4 h-4" />
        </button>
      </div>
    </header>
  );
};

