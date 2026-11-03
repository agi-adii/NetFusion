# NetFusion — Internet Bonding & Advanced Download Manager

[![Latest Release](https://img.shields.io/github/v/release/agi-adii/NetFusion?style=for-the-badge&label=Download%20Latest%20Release&color=38bdf8)](https://github.com/agi-adii/NetFusion/releases/latest)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011%20(64--bit)-6366f1?style=for-the-badge&logo=windows)](https://github.com/agi-adii/NetFusion/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-10b981?style=for-the-badge)](LICENSE)

**NetFusion** is a Windows 10 & Windows 11 desktop application built with **Tauri 2**, **Rust (Tokio)**, **React**, **TypeScript**, **Tailwind CSS**, **Framer Motion**, **Recharts**, and **SQLite** that combines multiple available internet connections (**Wi-Fi**, **Ethernet**, **Android/iPhone USB Tethering**, and secondary USB adapters) to maximize aggregate download bandwidth and connection resilience.

---

## Direct Download (For End Users — No Setup Required)

Anyone on **Windows 10 or Windows 11 (64-bit)** can download and run **NetFusion** immediately from the [**GitHub Releases page**](https://github.com/agi-adii/NetFusion/releases/latest):

1. **[Download `NetFusion-Setup-x64.exe` (1-Click Standalone Installer)](https://github.com/agi-adii/NetFusion/releases/download/v1.0.0/NetFusion-Setup-x64.exe)** — Single `.exe` file that automatically installs NetFusion, adds Desktop & Start Menu shortcuts, and launches the app immediately.
2. **[Download `NetFusion-v1.0.0-Windows-x64-Portable.zip` (Portable Archive)](https://github.com/agi-adii/NetFusion/releases/download/v1.0.0/NetFusion-v1.0.0-Windows-x64-Portable.zip)** — Extract anywhere and double-click **`NetFusion.exe`**.
3. **[Download `netfusion-bonding-server.exe`](https://github.com/agi-adii/NetFusion/releases/download/v1.0.0/netfusion-bonding-server.exe)** — *(Optional)* Standalone encrypted multipath bonding server binary.

---

## Key Capabilities

### 1. Multi-Network Internet Bonding Engine
- **Real Windows IP Helper Hardware Enumeration**: Detects Wi-Fi (`802.11`), Ethernet, Android RNDIS / CDC NCM USB Tethering, iPhone Apple Mobile Device USB Tethering, and WWAN adapters via Win32 `GetAdaptersAddresses` and `GetIfEntry2`.
- **Non-Intrusive Per-Socket Interface Pinning (`IP_UNICAST_IF`)**: Binds sockets to each adapter's IPv4 address **and** applies WinSock2 `setsockopt(IPPROTO_IP, IP_UNICAST_IF, htonl(if_index))`, bypassing Windows Weak Host default-route collapse without modifying system routing tables or requiring Administrator elevation.
- **5 Bonding Modes**:
  1. **Smart Bonding** — Dynamic EWMA throughput, RTT penalty, congestion cap, and loss damping.
  2. **Maximum Speed** — Saturates all active connections with dynamic straggler protection.
  3. **Balanced Mode** — Optimizes throughput, low latency, and link stability.
  4. **Failover Mode** — Uses your designated primary link and switches instantaneously on disconnect or >25% packet loss.
  5. **Custom Mode** — Manual per-interface traffic allocation sliders.
- **Encrypted Multipath Bonding Tunnel (`crates/netfusion-protocol` + `crates/bonding-server`)**:
  - Ephemeral **X25519 ECDH** + **HKDF-SHA256** + mutual **HMAC-SHA256** challenge (PSK never transmitted over wire; encrypted at rest in SQLite via ChaCha20-Poly1305) + **ChaCha20-Poly1305 AEAD** frame encryption.
  - Stripes logical TCP streams (`32 KiB` `StreamChunk`s) across all active physical NIC subflows and reassembles out-of-order chunks via a deterministic sliding-window `StreamReassembler` (`BTreeMap<u64, Vec<u8>>`).
  - Automatic subflow failover retransmission when any network disconnects mid-transfer.
  - Clear UI distinction when the Bonding Server is offline (**Direct Multi-Path HTTP Range Mode**).

### 2. Advanced Multi-Connection Download Manager
- Parallel byte-range (`206 Partial Content`) segmented downloads distributed across bonded interfaces.
- **Exact Byte-Offset Pause & Resume**: Stop/pause any download at any moment to immediately flush in-flight chunks to `<target>.nfdl` and checkpoint exact per-segment byte offsets to SQLite; resume seconds or days later from the exact byte where it stopped.
- **Dynamic Work-Stealing**: Fast interfaces split and steal remaining byte ranges (`>= 2 MiB`) from slower straggler segments so a slow network never stalls download completion.
- In-place `.nfdl` disk allocation (zero double-copy merge overhead on 20 GB+ files) with automatic crash/restart recovery.
- Priority queue (`Critical`, `High`, `Normal`, `Low`), download scheduling, global & per-task token-bucket speed limits, automatic retries across surviving links, and streaming `SHA-256` / `MD5` checksum verification.

### 3. Real-Time Network Monitor & Built-In Speed Test
- 1 Hz live bandwidth telemetry, separate Wi-Fi / Ethernet / USB Tethering graphs, per-interface RTT ping, jitter, packet loss, daily SQLite data usage tracking, unstable link detection, and connection history.
- Real internet speed test (Ping, Jitter, Packet Loss, Multi-Stream Download & Upload) supporting **Aggregate Bonding** and **Single-Interface** benchmarks.

### 4. Windows 10 & 11 Integration
- System tray icon & context menu (`Open`, `Pause All`, `Resume All`, `Exit`), background download operation on window close, Windows Startup autostart toggle, and native toast notifications.

---

## Project Structure

- [`crates/netfusion-protocol`](crates/netfusion-protocol) — Wire protocol, X25519/ChaCha20-Poly1305 crypto, multipath scheduler & stream reassembler
- [`crates/bonding-server`](crates/bonding-server) — Standalone VPS multipath bonding server (`netfusion-bonding-server`)
- [`src-tauri`](src-tauri) — Tauri 2 Rust backend (Win32 networking, bonding client, download engine, speed test, SQLite)
- [`src`](src) — React + TypeScript + Tailwind CSS + Framer Motion + Recharts desktop UI
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — Protocol specification & Windows networking limitations explained
- [`docs/SERVER_SETUP.md`](docs/SERVER_SETUP.md) — VPS deployment guide (`Docker Compose` & `systemd` + TCP BBR tuning)
- [`deploy/`](deploy) — Production `Dockerfile.bonding-server`, `docker-compose.yml`, and `netfusion-bonding.service`

---

## Build & Run Instructions (Windows 10 / 11)

```powershell
# 1. Install frontend dependencies
npm install

# 2. Run automated workspace unit & integration tests
cargo test --workspace

# 3. Build standalone Multipath Bonding Server binary
cargo build --release -p bonding-server

# 4. Run NetFusion in development mode
npx tauri dev

# 5. Build production Windows application & NSIS Setup Installer (.exe)
npx tauri build
```


