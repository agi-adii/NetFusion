use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{lookup_host, TcpListener, TcpSocket, TcpStream};

/// Opens a non-blocking Tokio `TcpStream` strictly pinned to a specific physical Windows adapter
/// using BOTH `bind(local_ipv4:0)` AND `setsockopt(IPPROTO_IP, IP_UNICAST_IF, htonl(if_index))`.
///
/// Uses `tokio::net::TcpSocket` so the Windows IOCP/AFD reactor registers the socket prior to
/// initiating `connect`, ensuring accurate sub-millisecond handshake completion without false timeouts.
pub async fn connect_tcp_on_interface(
    remote: SocketAddr,
    local_ipv4: Ipv4Addr,
    if_index: u32,
    timeout_dur: Duration,
) -> io::Result<TcpStream> {
    let remote_v4 = match remote {
        SocketAddr::V4(v4) => SocketAddr::V4(v4),
        SocketAddr::V6(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "IPv4 interface binding requires IPv4 remote address",
            ))
        }
    };

    let socket = TcpSocket::new_v4()?;
    socket.set_reuseaddr(true)?;
    let _ = socket.set_nodelay(true);

    // 1. Bind source IPv4 address of the selected physical adapter
    let bind_addr = SocketAddr::V4(SocketAddrV4::new(local_ipv4, 0));
    socket.bind(bind_addr)?;

    // 2. Pin outbound routing interface via WinSock2 IP_UNICAST_IF (network byte order!)
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawSocket;
        use windows_sys::Win32::Networking::WinSock::{setsockopt, IPPROTO_IP, IP_UNICAST_IF};

        let raw_sock = socket.as_raw_socket() as usize;
        let idx_network_order: u32 = if_index.to_be();
        let rc = unsafe {
            setsockopt(
                raw_sock,
                IPPROTO_IP,
                IP_UNICAST_IF,
                &idx_network_order as *const u32 as *const u8,
                std::mem::size_of::<u32>() as i32,
            )
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    let tokio_stream = tokio::time::timeout(timeout_dur, socket.connect(remote_v4))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Interface TCP connect timed out"))??;

    if let Some(err) = tokio_stream.take_error()? {
        return Err(err);
    }

    Ok(tokio_stream)
}

/// Resolves a host:port to an IPv4 `SocketAddr` and connects strictly over the specified interface.
pub async fn resolve_and_connect_on_interface(
    host: &str,
    port: u16,
    local_ipv4: Ipv4Addr,
    if_index: u32,
    timeout_dur: Duration,
) -> io::Result<TcpStream> {
    if local_ipv4.is_loopback() {
        return tokio::time::timeout(timeout_dur, TcpStream::connect((host, port)))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Connect timed out"))?;
    }

    let addrs: Vec<SocketAddr> = lookup_host((host, port)).await?.collect();
    let v4_target = addrs
        .into_iter()
        .find(|a| a.is_ipv4())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No IPv4 address for host"))?;

    // If target is localhost (e.g. local bonding-server test), connect normally
    if v4_target.ip().is_loopback() {
        return tokio::time::timeout(timeout_dur, TcpStream::connect(v4_target))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Local connect timed out"))?;
    }

    connect_tcp_on_interface(v4_target, local_ipv4, if_index, timeout_dur).await
}

/// Measures genuine per-interface internet RTT latency by performing a fast interface-pinned TCP
/// handshake to Cloudflare (`1.1.1.1:443`) or Google (`8.8.8.8:443` / `8.8.4.4:53`).
pub async fn probe_interface_rtt_ms(local_ipv4: Ipv4Addr, if_index: u32) -> Option<f64> {
    let targets = [
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 443),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 443),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 4, 4)), 53),
    ];

    for target in targets {
        let start = Instant::now();
        if let Ok(stream) =
            connect_tcp_on_interface(target, local_ipv4, if_index, Duration::from_millis(1200))
                .await
        {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            drop(stream);
            return Some(elapsed);
        }
    }
    None
}

/// Spawns an in-process localhost SOCKS5 bridge (`127.0.0.1:<ephemeral_port>`) whose outbound
/// TCP connections are strictly pinned to `(local_ipv4, if_index)` via `IP_UNICAST_IF`.
///
/// This allows `reqwest::Client` (`socks5h://127.0.0.1:<port>`) to use full Windows SChannel TLS,
/// HTTP/1.1 & HTTP/2, redirects, and streaming downloads pinned to any physical Windows NIC!
pub async fn spawn_interface_socks5_bridge(
    local_ipv4: Ipv4Addr,
    if_index: u32,
) -> io::Result<(u16, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let handle = tokio::spawn(async move {
        while let Ok((mut client, _)) = listener.accept().await {
            tokio::spawn(async move {
                let _ = handle_socks5_pinned(&mut client, local_ipv4, if_index).await;
            });
        }
    });

    Ok((port, handle))
}

async fn handle_socks5_pinned(
    client: &mut TcpStream,
    local_ipv4: Ipv4Addr,
    if_index: u32,
) -> io::Result<()> {
    let mut hdr = [0u8; 2];
    client.read_exact(&mut hdr).await?;
    if hdr[0] != 0x05 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Not SOCKS5"));
    }
    let nmethods = hdr[1] as usize;
    let mut methods = vec![0u8; nmethods];
    client.read_exact(&mut methods).await?;
    // No-auth reply (localhost only)
    client.write_all(&[0x05, 0x00]).await?;

    let mut req = [0u8; 4];
    client.read_exact(&mut req).await?;
    if req[1] != 0x01 {
        // Only CONNECT (0x01)
        client
            .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await?;
        return Ok(());
    }

    let host = match req[3] {
        0x01 => {
            let mut ip = [0u8; 4];
            client.read_exact(&mut ip).await?;
            Ipv4Addr::from(ip).to_string()
        }
        0x03 => {
            let mut len = [0u8; 1];
            client.read_exact(&mut len).await?;
            let mut domain = vec![0u8; len[0] as usize];
            client.read_exact(&mut domain).await?;
            String::from_utf8_lossy(&domain).to_string()
        }
        0x04 => {
            let mut ip6 = [0u8; 16];
            client.read_exact(&mut ip6).await?;
            Ipv6Addr::from(ip6).to_string()
        }
        _ => return Ok(()),
    };

    let mut port_buf = [0u8; 2];
    client.read_exact(&mut port_buf).await?;
    let port = u16::from_be_bytes(port_buf);

    match resolve_and_connect_on_interface(
        &host,
        port,
        local_ipv4,
        if_index,
        Duration::from_secs(12),
    )
    .await
    {
        Ok(mut outbound) => {
            client
                .write_all(&[0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0, 0])
                .await?;
            let _ = tokio::io::copy_bidirectional(client, &mut outbound).await;
        }
        Err(_) => {
            let _ = client
                .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await;
        }
    }
    Ok(())
}

