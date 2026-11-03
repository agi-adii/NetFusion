# NetFusion Bonding Server — VPS Deployment & Setup Guide

The **NetFusion Bonding Server** (`netfusion-bonding-server`) is a lightweight, high-throughput Rust daemon that aggregates multiple encrypted subflows from your Windows PC onto a fast cloud VPS link (1 Gbps – 10 Gbps).

---

## 1. Prerequisites

- A Linux VPS (Ubuntu 22.04 / 24.04, Debian 12, Rocky Linux 9, etc.) with a low-latency connection and high port speed (1 Gbps+ recommended).
- Inbound TCP port `9443` (or your chosen port) open in your cloud firewall / security group.
- A strong random Pre-Shared Key (32+ characters), generated via:
  ```bash
  openssl rand -hex 32
  ```

---

## 2. Option A: One-Command Docker Compose Deployment (Recommended)

1. Copy the repository (or `Cargo.toml`, `crates/`, and `deploy/`) onto your VPS:
   ```bash
   git clone <your-repo-url> netfusion
   cd netfusion/deploy
   ```

2. Export your secret Pre-Shared Key and launch:
   ```bash
   export NETFUSION_PSK="paste-your-64-hex-char-secret-here"
   docker compose up -d --build
   ```

3. Verify the server logs:
   ```bash
   docker logs -f netfusion-bonding-server
   ```

---

## 3. Option B: Native Linux Binary + `systemd` Deployment

1. Install Rust on the VPS and build the release binary:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
   source "$HOME/.cargo/env"
   cargo build --release --manifest-path crates/bonding-server/Cargo.toml
   sudo cp target/release/netfusion-bonding-server /usr/local/bin/
   ```

2. Create a dedicated unprivileged service user and environment config:
   ```bash
   sudo useradd -r -s /usr/sbin/nologin netfusion
   sudo mkdir -p /etc/netfusion
   sudo tee /etc/netfusion/bonding-server.env >/dev/null <<'EOF'
   NETFUSION_LISTEN=0.0.0.0:9443
   NETFUSION_PSK=replace-with-your-strong-secret-key
   EOF
   sudo chmod 600 /etc/netfusion/bonding-server.env
   sudo chown -R netfusion:netfusion /etc/netfusion
   ```

3. Install and enable the `systemd` unit:
   ```bash
   sudo cp deploy/netfusion-bonding.service /etc/systemd/system/
   sudo systemctl daemon-reload
   sudo systemctl enable --now netfusion-bonding
   sudo systemctl status netfusion-bonding
   ```

4. Allow port `9443/tcp` through UFW:
   ```bash
   sudo ufw allow 9443/tcp
   ```

---

## 4. Recommended Linux Kernel TCP BBR Tuning on VPS

Enabling **TCP BBR** congestion control and larger socket buffers significantly improves multipath bonding throughput over cellular/USB tethering links with variable RTT:

```bash
sudo tee /etc/sysctl.d/99-netfusion-bbr.conf >/dev/null <<'EOF'
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr
net.core.rmem_max = 33554432
net.core.wmem_max = 33554432
net.ipv4.tcp_rmem = 4096 87380 33554432
net.ipv4.tcp_wmem = 4096 65536 33554432
EOF
sudo sysctl --system
```

---

## 5. Connecting the NetFusion Windows Desktop App

1. Open **NetFusion** on Windows 10 or Windows 11.
2. Navigate to **Network Bonding** in the left sidebar.
3. Ensure your desired adapters (**Wi-Fi**, **Ethernet**, **USB Tethering**) are toggled **Enabled**.
4. Under **Multipath Encrypted Bonding Server**:
   - **Host : Port**: Enter `<YOUR_VPS_PUBLIC_IP>:9443`
   - **Pre-Shared Auth Key**: Paste your `NETFUSION_PSK` secret (stored encrypted at rest on Windows using ChaCha20-Poly1305).
5. Click **Connect Bonding Server**.
6. Within 1–2 seconds, you will see **Encrypted Tunnel Active (N Subflows)** in the top bar and `Tunnel Subflow Up` badges on each connected Windows network interface!

---

## 6. Local Loopback Testing (Without a Remote VPS)

You can also test the full multipath protocol locally on your Windows PC:

```powershell
.\target\release\netfusion-bonding-server.exe --listen 127.0.0.1:9443 --psk my-test-key --allow-local-targets
```
Then connect NetFusion to `127.0.0.1:9443` with key `my-test-key`.

