# NetFusion Architecture & Windows Networking Engineering Guide

## 1. System Architecture Overview

NetFusion is organized as a Rust + TypeScript workspace comprising three core components:

1. **`crates/netfusion-protocol`** — Shared binary wire protocol, cryptography, multipath packet/chunk scheduler, and out-of-order byte-offset stream reassembler.
2. **`crates/bonding-server`** — Standalone cloud VPS bonding endpoint (`netfusion-bonding-server`) written in asynchronous Rust (`tokio`) that terminates multiple encrypted subflows from a client and reassembles/stripes traffic to and from the internet.
3. **`src-tauri` + `src/`** — Native Windows 10/11 desktop application built on **Tauri 2**, **Rust**, **Tokio**, **SQLite**, **React**, **TypeScript**, **Tailwind CSS**, **Framer Motion**, and **Recharts**.

---

## 2. How Real Multipath Internet Bonding Works in NetFusion

Why can't an application combine two internet connections on a single TCP socket without a bonding endpoint?
Because an ordinary TCP connection is identified by a 4-tuple `(client_public_ip, client_port, server_ip, server_port)`. Your Wi-Fi ISP and your USB Tethering cellular carrier assign completely different public IP addresses. If packets from a single normal TCP socket were sent out of two different ISPs, the destination server would reject packets from the second IP, or ISP Reverse-Path Filtering (`uRPF`) would drop spoofed source IPs.

NetFusion solves this honestly and transparently using a **two-tier multipath architecture**:

### Tier 1: Full Encrypted Multipath Tunnel Bonding (`Client <-> NetFusion Bonding Server`)
When connected to a **NetFusion Bonding Server** (hosted on a high-bandwidth VPS):
1. **Per-NIC Subflow Establishment**: For every enabled, connected Windows network interface (e.g., `Wi-Fi` + `Ethernet` + `USB Tethering`), NetFusion opens a dedicated TCP subflow pinned strictly to that physical adapter (`bind(adapter_ipv4)` + `setsockopt(IPPROTO_IP, IP_UNICAST_IF, htonl(if_index))`).
2. **Mutual Authentication & AEAD Encryption (`NFB1` Protocol)**:
   - Client and server perform an **ephemeral X25519 Diffie-Hellman** key exchange.
   - Client proves possession of the Pre-Shared Key (PSK) via `HMAC-SHA256` over the session ID, subflow ID, public key, nonce, and timestamp — **the secret key is never transmitted over the network**.
   - Directional 256-bit keys are derived via `HKDF-SHA256`, and the server returns a mutual verification tag so MITM endpoints cannot impersonate the server.
   - Every frame is encrypted and authenticated with **ChaCha20-Poly1305 AEAD** using monotonic 64-bit sequence nonces.
3. **Multipath Striping, Scheduling & Reassembly**:
   - Logical streams (`TcpOpen`) are sliced into `32 KiB` `StreamChunk { stream_id, offset, eof, data }` binary frames.
   - Both client and server run `MultipathScheduler` (Weighted Deficit Round-Robin driven by the active `BondingPolicy`: `Smart`, `MaxSpeed`, `Balanced`, `Failover`, or `Custom`) to dispatch chunks simultaneously across all active subflows according to real-time throughput, RTT latency, and packet loss.
   - Because heterogeneous networks have different latencies (e.g., 12 ms Ethernet vs. 45 ms 5G USB Tethering), chunks arrive out of order. `StreamReassembler` (`BTreeMap<u64, Vec<u8>>`) orders chunks by exact `offset` and delivers a lossless, ordered byte stream.
   - **Instant Subflow Failover**: If an interface disconnects mid-transfer, any frame queued or failing on that subflow is immediately re-dispatched over surviving subflows without breaking the logical TCP stream.

### Tier 2: Direct Multi-Interface Segmented HTTP Acceleration (No VPS Required)
When no Bonding Server is configured or reachable, NetFusion **explicitly indicates** `Direct Multi-Path Mode` (or `Bonding Server Unavailable`) in the UI and never pretends a tunnel is active.
- For HTTP/HTTPS servers supporting `Accept-Ranges: bytes` (`206 Partial Content`), the Download Engine splits files into parallel byte-range segments and routes concurrent segment workers through **separate per-interface `IP_UNICAST_IF`-pinned HTTP clients** across Wi-Fi, Ethernet, and USB Tethering simultaneously!
- **Dynamic Work-Stealing (Straggler Protection)**: When a fast interface finishes its segment while a slower interface still has `>= 2 MiB` left, the fast worker dynamically splits the active segment at its midpoint and steals the second half so a slow connection never bottlenecks completion.

---

## 3. Windows 10 & Windows 11 Networking Limitations & Design Decisions

1. **Windows Weak Host Model vs. `IP_UNICAST_IF`**:
   - By default, Windows 10 and 11 use the *Weak Host Model*. Calling `bind(adapter_b_ipv4)` alone does **not** force Windows to transmit packets out of Adapter B — Windows still consults the `0.0.0.0/0` default route with the lowest metric (Adapter A), sending packets with Adapter B's source IP out of Adapter A's gateway (where Adapter A's ISP drops them).
   - NetFusion uses WinSock2 `setsockopt(sock, IPPROTO_IP, IP_UNICAST_IF, &if_index.to_be(), 4)` in addition to `bind(local_ipv4)`. `IP_UNICAST_IF` is an unprivileged per-socket Windows API option that instructs `tcpip.sys` to route that socket strictly through the specified interface index.
   - **Zero System Mutation**: NetFusion never alters global Windows routing tables (`route add`), adapter metrics, registry settings, or system proxy settings. Exiting NetFusion leaves Windows networking 100% untouched.

2. **Single Wi-Fi Radio Hardware Constraint**:
   - A single physical Wi-Fi card can associate with only **one** Wi-Fi access point (SSID) at a time. To bond two Wi-Fi networks simultaneously, connect a secondary USB Wi-Fi adapter alongside your built-in Wi-Fi card, or combine Wi-Fi + Ethernet + Smartphone USB Tethering.

3. **Android & iPhone USB Tethering on Windows**:
   - **Android**: Plug in via USB and enable *USB Tethering* in Android Settings. Windows loads the built-in `Remote NDIS` (`RNDIS`) or `UsbNcm` driver automatically; NetFusion detects it immediately as `USB Tethering`.
   - **iPhone (iOS)**: Requires the *Apple Mobile Device Ethernet* driver (installed automatically when Apple's *iTunes* or *Apple Devices* app from the Microsoft Store is installed). Once Personal Hotspot via USB is active, NetFusion detects it as `USB Tethering`.

4. **Single-Stream Non-Range Servers Without a Bonding Server**:
   - If a web server disables HTTP byte ranges (`Accept-Ranges: none`) **and** you are not connected to a NetFusion Bonding Server, a single HTTP stream can only traverse one physical interface at a time (with automatic retry/failover). Connect to a NetFusion Bonding Server to stripe even non-Range single streams across all interfaces.

