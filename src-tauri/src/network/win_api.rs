use crate::models::InterfaceKind;
use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone)]
pub struct RawAdapterSnapshot {
    pub id: String,
    pub if_index: u32,
    pub name: String,
    pub description: String,
    pub kind: InterfaceKind,
    pub is_up: bool,
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
    pub gateway: Option<Ipv4Addr>,
    pub mac_address: String,
    pub link_speed_bps: u64,
    pub rx_octets: u64,
    pub tx_octets: u64,
    pub rx_errors: u64,
}

#[cfg(windows)]
pub fn enumerate_windows_adapters() -> Vec<RawAdapterSnapshot> {
    use std::ffi::CStr;
    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GetIfEntry2, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_INCLUDE_PREFIX,
        IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211, IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL,
        IP_ADAPTER_ADDRESSES_LH, MIB_IF_ROW2,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows_sys::Win32::Networking::WinSock::{
        AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_IN, SOCKADDR_IN6,
    };

    let flags = GAA_FLAG_INCLUDE_PREFIX | GAA_FLAG_INCLUDE_GATEWAYS;
    let mut buf_len: u32 = 32 * 1024;
    let mut buffer: Vec<u8> = vec![0u8; buf_len as usize];

    unsafe {
        let mut ret = GetAdaptersAddresses(
            AF_UNSPEC as u32,
            flags,
            std::ptr::null(),
            buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
            &mut buf_len,
        );
        if ret == ERROR_BUFFER_OVERFLOW {
            buffer.resize(buf_len as usize + 4096, 0);
            ret = GetAdaptersAddresses(
                AF_UNSPEC as u32,
                flags,
                std::ptr::null(),
                buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                &mut buf_len,
            );
        }
        if ret != NO_ERROR {
            return Vec::new();
        }

        let mut results = Vec::new();
        let mut curr = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;

        while !curr.is_null() {
            let adapter = &*curr;
            let if_type = adapter.IfType;

            // Skip software loopback and raw IP tunnels
            if if_type != IF_TYPE_SOFTWARE_LOOPBACK && if_type != IF_TYPE_TUNNEL {
                let if_index = adapter.Anonymous1.Anonymous.IfIndex;
                let id = if !adapter.AdapterName.is_null() {
                    CStr::from_ptr(adapter.AdapterName as *const i8)
                        .to_string_lossy()
                        .to_string()
                } else {
                    format!("if-{if_index}")
                };

                let name = pwstr_to_string(adapter.FriendlyName);
                let description = pwstr_to_string(adapter.Description);
                let is_up = adapter.OperStatus == IfOperStatusUp;

                // Filter out Windows internal virtual miniports/filters that never carry user internet
                if !is_noise_miniport(&name, &description) {
                    let mut ipv4: Option<Ipv4Addr> = None;
                    let mut ipv6: Option<Ipv6Addr> = None;

                    let mut uni = adapter.FirstUnicastAddress;
                    while !uni.is_null() {
                        let u = &*uni;
                        let sa = u.Address.lpSockaddr;
                        if !sa.is_null() {
                            let fam = (*sa).sa_family;
                            if fam == AF_INET && ipv4.is_none() {
                                let sin = &*(sa as *const SOCKADDR_IN);
                                let bytes = sin.sin_addr.S_un.S_addr.to_ne_bytes();
                                let candidate = Ipv4Addr::from(bytes);
                                if !candidate.is_loopback()
                                    && !candidate.is_unspecified()
                                    && !candidate.is_link_local()
                                {
                                    ipv4 = Some(candidate);
                                }
                            } else if fam == AF_INET6 && ipv6.is_none() {
                                let sin6 = &*(sa as *const SOCKADDR_IN6);
                                let segs = sin6.sin6_addr.u.Byte;
                                let candidate = Ipv6Addr::from(segs);
                                if !candidate.is_loopback()
                                    && !candidate.is_unspecified()
                                    && !candidate.is_unicast_link_local()
                                {
                                    ipv6 = Some(candidate);
                                }
                            }
                        }
                        uni = u.Next;
                    }

                    let mut gateway: Option<Ipv4Addr> = None;
                    let mut gw = adapter.FirstGatewayAddress;
                    while !gw.is_null() {
                        let g = &*gw;
                        let sa = g.Address.lpSockaddr;
                        if !sa.is_null() && (*sa).sa_family == AF_INET {
                            let sin = &*(sa as *const SOCKADDR_IN);
                            let bytes = sin.sin_addr.S_un.S_addr.to_ne_bytes();
                            let candidate = Ipv4Addr::from(bytes);
                            if !candidate.is_unspecified() {
                                gateway = Some(candidate);
                                break;
                            }
                        }
                        gw = g.Next;
                    }

                    let mac_len = (adapter.PhysicalAddressLength as usize).min(8);
                    let mac_address = if mac_len > 0 {
                        adapter.PhysicalAddress[..mac_len]
                            .iter()
                            .map(|b| format!("{b:02X}"))
                            .collect::<Vec<_>>()
                            .join(":")
                    } else {
                        "00:00:00:00:00:00".to_string()
                    };

                    let raw_speed = adapter.ReceiveLinkSpeed.max(adapter.TransmitLinkSpeed);
                    let link_speed_bps = if raw_speed == u64::MAX { 0 } else { raw_speed };

                    // Query 64-bit hardware byte & error counters via GetIfEntry2
                    let mut if_row: MIB_IF_ROW2 = std::mem::zeroed();
                    if_row.InterfaceIndex = if_index;
                    let (rx_octets, tx_octets, rx_errors) =
                        if GetIfEntry2(&mut if_row) == NO_ERROR {
                            (
                                if_row.InOctets,
                                if_row.OutOctets,
                                if_row.InErrors + if_row.InDiscards,
                            )
                        } else {
                            (0, 0, 0)
                        };

                    let kind = classify_interface(
                        if_type,
                        &name,
                        &description,
                        if_type == IF_TYPE_IEEE80211,
                        if_type == IF_TYPE_ETHERNET_CSMACD,
                    );

                    // Keep physical adapters + any connected/IP-bearing virtual/USB adapters
                    if mac_len > 0 || is_up || ipv4.is_some() {
                        results.push(RawAdapterSnapshot {
                            id,
                            if_index,
                            name,
                            description,
                            kind,
                            is_up,
                            ipv4,
                            ipv6,
                            gateway,
                            mac_address,
                            link_speed_bps,
                            rx_octets,
                            tx_octets,
                            rx_errors,
                        });
                    }
                }
            }
            curr = adapter.Next;
        }

        // Sort connected interfaces with gateways first, then Wi-Fi/Ethernet/USB
        results.sort_by(|a, b| {
            let a_active = a.is_up && a.ipv4.is_some();
            let b_active = b.is_up && b.ipv4.is_some();
            b_active
                .cmp(&a_active)
                .then_with(|| b.gateway.is_some().cmp(&a.gateway.is_some()))
                .then_with(|| a.name.cmp(&b.name))
        });

        results
    }
}

#[cfg(not(windows))]
pub fn enumerate_windows_adapters() -> Vec<RawAdapterSnapshot> {
    Vec::new()
}

unsafe fn pwstr_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    String::from_utf16_lossy(slice)
}

fn is_noise_miniport(name: &str, desc: &str) -> bool {
    let d = desc.to_lowercase();
    let n = name.to_lowercase();
    d.contains("wan miniport")
        || d.contains("kernel debug")
        || d.contains("teredo")
        || d.contains("isatap")
        || d.contains("6to4")
        || d.contains("wi-fi direct virtual")
        || d.contains("microsoft hosted network")
        || n.contains("local area connection* ")
}

fn classify_interface(
    if_type: u32,
    name: &str,
    desc: &str,
    is_80211: bool,
    is_eth: bool,
) -> InterfaceKind {
    let combined = format!("{} {}", name.to_lowercase(), desc.to_lowercase());

    // Android RNDIS / CDC NCM & iPhone Apple Mobile Device USB Tethering detection
    if combined.contains("rndis")
        || combined.contains("remote ndis")
        || combined.contains("apple mobile device")
        || combined.contains("internet sharing")
        || combined.contains("tether")
        || combined.contains("usb ethernet")
        || combined.contains("usb lan")
        || combined.contains("cdc")
        || combined.contains("usbn")
    {
        return InterfaceKind::UsbTethering;
    }

    // WWAN / Cellular modem
    if if_type == 243 || if_type == 244 || combined.contains("cellular") || combined.contains("wwan")
    {
        return InterfaceKind::Cellular;
    }

    if is_80211 || combined.contains("wi-fi") || combined.contains("wlan") || combined.contains("wireless") {
        return InterfaceKind::WiFi;
    }

    if combined.contains("hyper-v")
        || combined.contains("vmware")
        || combined.contains("virtualbox")
        || combined.contains("vethernet")
        || combined.contains("tap-windows")
        || combined.contains("wintun")
        || combined.contains("tailscale")
        || combined.contains("zerotier")
    {
        return InterfaceKind::Virtual;
    }

    if is_eth {
        return InterfaceKind::Ethernet;
    }

    InterfaceKind::Other
}

