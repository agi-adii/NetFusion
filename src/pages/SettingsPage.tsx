import React, { useState, useEffect } from "react";
import {
  FolderOpen,
  Save,
  Shield,
  Monitor,
  Download,
  Server,
  CheckCircle2,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { isTauriRuntime } from "../lib/tauri";
import type { AppSettings, UpdateSettingsPayload } from "../lib/types";

interface SettingsPageProps {
  settings: AppSettings;
  onSave: (payload: UpdateSettingsPayload) => Promise<void>;
}

export const SettingsPage: React.FC<SettingsPageProps> = ({ settings, onSave }) => {
  const [form, setForm] = useState<UpdateSettingsPayload>({
    default_download_dir: settings.default_download_dir,
    max_concurrent_downloads: settings.max_concurrent_downloads,
    default_segments_per_download: settings.default_segments_per_download,
    adaptive_concurrency: settings.adaptive_concurrency,
    global_speed_limit_bps: settings.global_speed_limit_bps,
    auto_retry_downloads: settings.auto_retry_downloads,
    max_retries: settings.max_retries,
    bonding_mode: settings.bonding_mode,
    bonding_tunnel_enabled: settings.bonding_tunnel_enabled,
    bonding_server_endpoint: settings.bonding_server_endpoint,
    new_bonding_auth_token: null,
    enable_local_socks_proxy: settings.enable_local_socks_proxy,
    local_socks_port: settings.local_socks_port,
    launch_at_startup: settings.launch_at_startup,
    minimize_to_tray: settings.minimize_to_tray,
    run_in_background: settings.run_in_background,
    notify_on_complete: settings.notify_on_complete,
    theme: settings.theme,
    speedtest_endpoint: settings.speedtest_endpoint,
  });

  const [speedLimitMB, setSpeedLimitMB] = useState<string>(
    settings.global_speed_limit_bps
      ? (settings.global_speed_limit_bps / (8 * 1024 * 1024)).toFixed(1)
      : ""
  );
  const [newToken, setNewToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [savedToast, setSavedToast] = useState(false);

  useEffect(() => {
    setForm({
      default_download_dir: settings.default_download_dir,
      max_concurrent_downloads: settings.max_concurrent_downloads,
      default_segments_per_download: settings.default_segments_per_download,
      adaptive_concurrency: settings.adaptive_concurrency,
      global_speed_limit_bps: settings.global_speed_limit_bps,
      auto_retry_downloads: settings.auto_retry_downloads,
      max_retries: settings.max_retries,
      bonding_mode: settings.bonding_mode,
      bonding_tunnel_enabled: settings.bonding_tunnel_enabled,
      bonding_server_endpoint: settings.bonding_server_endpoint,
      new_bonding_auth_token: null,
      enable_local_socks_proxy: settings.enable_local_socks_proxy,
      local_socks_port: settings.local_socks_port,
      launch_at_startup: settings.launch_at_startup,
      minimize_to_tray: settings.minimize_to_tray,
      run_in_background: settings.run_in_background,
      notify_on_complete: settings.notify_on_complete,
      theme: settings.theme,
      speedtest_endpoint: settings.speedtest_endpoint,
    });
    setSpeedLimitMB(
      settings.global_speed_limit_bps
        ? (settings.global_speed_limit_bps / (8 * 1024 * 1024)).toFixed(1)
        : ""
    );
  }, [settings]);

  const handleBrowse = async () => {
    if (!isTauriRuntime()) return;
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: form.default_download_dir || undefined,
      title: "Select Default Download Folder",
    });
    if (typeof selected === "string") {
      setForm((prev) => ({ ...prev, default_download_dir: selected }));
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setSavedToast(false);

    const parsedLimit = parseFloat(speedLimitMB);
    const limitBps =
      speedLimitMB.trim() && !isNaN(parsedLimit) && parsedLimit > 0
        ? Math.round(parsedLimit * 8 * 1024 * 1024)
        : null;

    try {
      await onSave({
        ...form,
        global_speed_limit_bps: limitBps,
        new_bonding_auth_token: newToken.trim() ? newToken.trim() : null,
      });
      setNewToken("");
      setSavedToast(true);
      setTimeout(() => setSavedToast(false), 3000);
    } finally {
      setSaving(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-5 pb-8 max-w-4xl">
      {/* Download Acceleration Settings */}
      <div className="glass-card rounded-2xl p-5 space-y-4">
        <div className="flex items-center gap-2.5">
          <Download className="w-4 h-4 text-electric-400" />
          <h2 className="text-sm font-bold text-slate-900 dark:text-white">
            Download Engine & Acceleration
          </h2>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="md:col-span-2">
            <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
              Default Download Directory
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={form.default_download_dir}
                onChange={(e) =>
                  setForm({ ...form, default_download_dir: e.target.value })
                }
                className="flex-1 px-3.5 py-2 rounded-xl text-xs bg-slate-100 dark:bg-navy-950 border border-slate-300/70 dark:border-white/10 text-slate-900 dark:text-white"
              />
              <button
                type="button"
                onClick={handleBrowse}
                className="px-3.5 py-2 rounded-xl glass-card text-xs font-semibold flex items-center gap-1.5"
              >
                <FolderOpen className="w-3.5 h-3.5 text-electric-400" />
                Browse
              </button>
            </div>
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
              Simultaneous Downloads ({form.max_concurrent_downloads})
            </label>
            <input
              type="range"
              min={1}
              max={12}
              value={form.max_concurrent_downloads}
              onChange={(e) =>
                setForm({ ...form, max_concurrent_downloads: Number(e.target.value) })
              }
              className="w-full accent-electric-500"
            />
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
              Default Segments per File ({form.default_segments_per_download})
            </label>
            <input
              type="range"
              min={1}
              max={32}
              value={form.default_segments_per_download}
              onChange={(e) =>
                setForm({
                  ...form,
                  default_segments_per_download: Number(e.target.value),
                })
              }
              className="w-full accent-lavender-500"
            />
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
              Global Download Speed Limit (MB/s)
            </label>
            <input
              type="number"
              step="0.5"
              min="0"
              value={speedLimitMB}
              onChange={(e) => setSpeedLimitMB(e.target.value)}
              placeholder="Unlimited"
              className="w-full px-3.5 py-2 rounded-xl text-xs bg-slate-100 dark:bg-navy-950 border border-slate-300/70 dark:border-white/10"
            />
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
              Max Automatic Retries ({form.max_retries})
            </label>
            <input
              type="number"
              min={0}
              max={20}
              value={form.max_retries}
              onChange={(e) => setForm({ ...form, max_retries: Number(e.target.value) })}
              className="w-full px-3.5 py-2 rounded-xl text-xs bg-slate-100 dark:bg-navy-950 border border-slate-300/70 dark:border-white/10"
            />
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3 pt-1">
          <ToggleRow
            label="Adaptive Concurrency & Dynamic Work-Stealing"
            desc="Scales connections based on bandwidth headroom and splits slow straggler segments"
            checked={form.adaptive_concurrency}
            onChange={(v) => setForm({ ...form, adaptive_concurrency: v })}
          />
          <ToggleRow
            label="Automatic Segment & Download Retry"
            desc="Retries interrupted segments automatically across surviving active interfaces"
            checked={form.auto_retry_downloads}
            onChange={(v) => setForm({ ...form, auto_retry_downloads: v })}
          />
        </div>
      </div>

      {/* Bonding Server & Security */}
      <div className="glass-card rounded-2xl p-5 space-y-4">
        <div className="flex items-center gap-2.5">
          <Server className="w-4 h-4 text-lavender-400" />
          <h2 className="text-sm font-bold text-slate-900 dark:text-white">
            Bonding Server & Encrypted Tunnel Security
          </h2>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
              Bonding Server Endpoint
            </label>
            <input
              type="text"
              value={form.bonding_server_endpoint}
              onChange={(e) =>
                setForm({ ...form, bonding_server_endpoint: e.target.value })
              }
              placeholder="127.0.0.1:9443 or your-vps.example.com:9443"
              className="w-full px-3.5 py-2 rounded-xl text-xs font-mono bg-slate-100 dark:bg-navy-950 border border-slate-300/70 dark:border-white/10"
            />
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
              Rotate Pre-Shared Auth Key ({settings.bonding_auth_token_preview})
            </label>
            <input
              type="password"
              value={newToken}
              onChange={(e) => setNewToken(e.target.value)}
              placeholder="Stored hashed/encrypted via OS key derivation"
              className="w-full px-3.5 py-2 rounded-xl text-xs font-mono bg-slate-100 dark:bg-navy-950 border border-slate-300/70 dark:border-white/10"
            />
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
              Opt-In Local Bonded SOCKS5 Proxy Port
            </label>
            <input
              type="number"
              min={1024}
              max={65535}
              value={form.local_socks_port}
              onChange={(e) =>
                setForm({ ...form, local_socks_port: Number(e.target.value) })
              }
              className="w-full px-3.5 py-2 rounded-xl text-xs font-mono bg-slate-100 dark:bg-navy-950 border border-slate-300/70 dark:border-white/10"
            />
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5">
              Speed Test Target Endpoint
            </label>
            <input
              type="text"
              value={form.speedtest_endpoint}
              onChange={(e) =>
                setForm({ ...form, speedtest_endpoint: e.target.value })
              }
              className="w-full px-3.5 py-2 rounded-xl text-xs font-mono bg-slate-100 dark:bg-navy-950 border border-slate-300/70 dark:border-white/10"
            />
          </div>
        </div>

        <ToggleRow
          label="Expose Local Bonded SOCKS5 Proxy (127.0.0.1 only)"
          desc="Allows external apps you explicitly configure to use the bonded tunnel; never modifies Windows system proxy"
          checked={form.enable_local_socks_proxy}
          onChange={(v) => setForm({ ...form, enable_local_socks_proxy: v })}
        />
      </div>

      {/* Windows 10 / 11 Integration */}
      <div className="glass-card rounded-2xl p-5 space-y-4">
        <div className="flex items-center gap-2.5">
          <Monitor className="w-4 h-4 text-emerald-400" />
          <h2 className="text-sm font-bold text-slate-900 dark:text-white">
            Windows 10 & Windows 11 Integration
          </h2>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <ToggleRow
            label="Launch NetFusion on Windows Startup"
            desc="Registers safe user-level autostart entry without requiring Administrator elevation"
            checked={form.launch_at_startup}
            onChange={(v) => setForm({ ...form, launch_at_startup: v })}
          />
          <ToggleRow
            label="Minimize to System Tray on Close"
            desc="Keeps active downloads running seamlessly in the background tray icon"
            checked={form.minimize_to_tray}
            onChange={(v) => setForm({ ...form, minimize_to_tray: v })}
          />
          <ToggleRow
            label="Desktop Notifications on Download Completion"
            desc="Sends native Windows toast notifications when downloads finish or fail"
            checked={form.notify_on_complete}
            onChange={(v) => setForm({ ...form, notify_on_complete: v })}
          />
          <ToggleRow
            label="Dark Navy Glassmorphism Theme"
            desc="Toggle between Dark Navy and Fluent Light Mica appearance"
            checked={form.theme === "dark"}
            onChange={(v) => setForm({ ...form, theme: v ? "dark" : "light" })}
          />
        </div>

        <div className="p-3 rounded-xl bg-emerald-500/10 border border-emerald-400/25 flex items-center gap-2.5 text-xs text-emerald-700 dark:text-emerald-300">
          <Shield className="w-4 h-4 shrink-0" />
          <span>
            <strong>Zero Global Network Mutation Guarantee:</strong> NetFusion binds sockets per-connection via WinSock2{" "}
            <code>IP_UNICAST_IF</code> and never alters Windows adapter settings, DNS, firewall rules, or system proxy configuration.
          </span>
        </div>
      </div>

      <div className="flex items-center justify-between">
        <div>
          {savedToast && (
            <span className="inline-flex items-center gap-1.5 text-xs font-semibold text-emerald-400">
              <CheckCircle2 className="w-4 h-4" />
              Settings saved and applied
            </span>
          )}
        </div>
        <button
          type="submit"
          disabled={saving}
          className="inline-flex items-center gap-2 px-6 py-2.5 rounded-xl text-xs font-semibold text-white bg-gradient-to-r from-electric-500 via-indigo-500 to-lavender-500 hover:brightness-110 shadow-glow-blue"
        >
          <Save className="w-4 h-4" />
          {saving ? "Saving..." : "Save Settings"}
        </button>
      </div>
    </form>
  );
};

const ToggleRow: React.FC<{
  label: string;
  desc: string;
  checked: boolean;
  onChange: (val: boolean) => void;
}> = ({ label, desc, checked, onChange }) => (
  <label className="p-3 rounded-xl bg-slate-100/70 dark:bg-white/[0.03] border border-slate-200/70 dark:border-white/[0.07] flex items-center justify-between gap-3 cursor-pointer">
    <div>
      <div className="text-xs font-semibold text-slate-900 dark:text-white">{label}</div>
      <div className="text-[11px] text-slate-500 dark:text-slate-400">{desc}</div>
    </div>
    <div className="relative shrink-0">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="sr-only peer"
      />
      <div className="w-10 h-5 bg-slate-300 dark:bg-slate-700 rounded-full peer peer-checked:bg-gradient-to-r peer-checked:from-electric-500 peer-checked:to-lavender-500 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:after:translate-x-5" />
    </div>
  </label>
);

