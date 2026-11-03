import React from "react";
import {
  Wifi,
  Network,
  Smartphone,
  Radio,
  Cpu,
  Globe,
  FileArchive,
  FileVideo,
  FileAudio,
  FileText,
  HardDrive,
  Package,
  File,
} from "lucide-react";
import type { InterfaceKind, InterfaceStatus } from "../lib/types";
import clsx from "clsx";

export const InterfaceIcon: React.FC<{ kind: InterfaceKind; className?: string }> = ({
  kind,
  className = "w-4 h-4",
}) => {
  switch (kind) {
    case "WiFi":
      return <Wifi className={className} />;
    case "Ethernet":
      return <Network className={className} />;
    case "UsbTethering":
      return <Smartphone className={className} />;
    case "Cellular":
      return <Radio className={className} />;
    case "Virtual":
      return <Cpu className={className} />;
    default:
      return <Globe className={className} />;
  }
};

export function interfaceKindLabel(kind: InterfaceKind): string {
  switch (kind) {
    case "WiFi":
      return "Wi-Fi";
    case "Ethernet":
      return "Ethernet";
    case "UsbTethering":
      return "USB Tethering";
    case "Cellular":
      return "Cellular";
    case "Virtual":
      return "Virtual NIC";
    default:
      return "Network";
  }
}

export function interfaceKindColors(kind: InterfaceKind): {
  badge: string;
  accentHex: string;
} {
  switch (kind) {
    case "WiFi":
      return {
        badge: "bg-electric-500/15 text-electric-400 border-electric-400/30",
        accentHex: "#38bdf8",
      };
    case "Ethernet":
      return {
        badge: "bg-lavender-500/15 text-lavender-400 border-lavender-400/30",
        accentHex: "#a78bfa",
      };
    case "UsbTethering":
      return {
        badge: "bg-emerald-500/15 text-emerald-400 border-emerald-400/30",
        accentHex: "#34d399",
      };
    case "Cellular":
      return {
        badge: "bg-amber-500/15 text-amber-400 border-amber-400/30",
        accentHex: "#fbbf24",
      };
    default:
      return {
        badge: "bg-slate-500/15 text-slate-300 border-slate-400/30",
        accentHex: "#94a3b8",
      };
  }
}

export const StatusPill: React.FC<{ status: InterfaceStatus }> = ({ status }) => {
  const styles: Record<InterfaceStatus, string> = {
    Connected: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30",
    Unstable: "bg-amber-500/15 text-amber-400 border-amber-500/30",
    Disconnected: "bg-rose-500/15 text-rose-400 border-rose-500/30",
    Disabled: "bg-slate-500/15 text-slate-400 border-slate-500/30",
  };
  const dot: Record<InterfaceStatus, string> = {
    Connected: "bg-emerald-400",
    Unstable: "bg-amber-400 animate-pulse",
    Disconnected: "bg-rose-400",
    Disabled: "bg-slate-400",
  };
  return (
    <span
      className={clsx(
        "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[11px] font-medium border",
        styles[status]
      )}
    >
      <span className={clsx("w-1.5 h-1.5 rounded-full", dot[status])} />
      {status}
    </span>
  );
};

export const FileTypeIcon: React.FC<{ fileName: string; className?: string }> = ({
  fileName,
  className = "w-5 h-5",
}) => {
  const ext = fileName.split(".").pop()?.toLowerCase() || "";
  if (["zip", "rar", "7z", "tar", "gz", "xz", "zst"].includes(ext)) {
    return (
      <div className="p-2.5 rounded-xl bg-amber-500/15 text-amber-400 border border-amber-400/25">
        <FileArchive className={className} />
      </div>
    );
  }
  if (["iso", "img", "vhd", "vhdx", "dmg"].includes(ext)) {
    return (
      <div className="p-2.5 rounded-xl bg-lavender-500/15 text-lavender-400 border border-lavender-400/25">
        <HardDrive className={className} />
      </div>
    );
  }
  if (["exe", "msi", "msix", "appx"].includes(ext)) {
    return (
      <div className="p-2.5 rounded-xl bg-electric-500/15 text-electric-400 border border-electric-400/25">
        <Package className={className} />
      </div>
    );
  }
  if (["mp4", "mkv", "webm", "mov", "avi"].includes(ext)) {
    return (
      <div className="p-2.5 rounded-xl bg-pink-500/15 text-pink-400 border border-pink-400/25">
        <FileVideo className={className} />
      </div>
    );
  }
  if (["mp3", "flac", "wav", "ogg", "m4a"].includes(ext)) {
    return (
      <div className="p-2.5 rounded-xl bg-emerald-500/15 text-emerald-400 border border-emerald-400/25">
        <FileAudio className={className} />
      </div>
    );
  }
  if (["pdf", "doc", "docx", "txt", "md", "csv", "json"].includes(ext)) {
    return (
      <div className="p-2.5 rounded-xl bg-sky-500/15 text-sky-400 border border-sky-400/25">
        <FileText className={className} />
      </div>
    );
  }
  return (
    <div className="p-2.5 rounded-xl bg-slate-500/15 text-slate-300 border border-slate-400/25">
      <File className={className} />
    </div>
  );
};

