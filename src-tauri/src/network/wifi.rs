use crate::models::WifiNetworkScanItem;
use std::collections::{HashMap, HashSet};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Default)]
pub struct WifiAdapterRuntimeInfo {
    pub interface_name: String,
    pub connected: bool,
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub signal_pct: Option<u8>,
    pub radio_type: Option<String>,
    pub profile: Option<String>,
}

fn hidden_netsh(args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("netsh");
    cmd.args(args);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Queries Windows Wireless LAN (`netsh wlan show interfaces`) for all physical Wi-Fi adapters
/// (`Wi-Fi`, `Wi-Fi 2`, etc.) and returns a map keyed by lowercase adapter FriendlyName.
pub fn query_windows_wifi_interfaces() -> HashMap<String, WifiAdapterRuntimeInfo> {
    let Some(stdout) = hidden_netsh(&["wlan", "show", "interfaces"]) else {
        return HashMap::new();
    };
    parse_netsh_wlan_interfaces(&stdout)
}

pub fn parse_netsh_wlan_interfaces(stdout: &str) -> HashMap<String, WifiAdapterRuntimeInfo> {
    let mut map = HashMap::new();
    let mut current: Option<WifiAdapterRuntimeInfo> = None;

    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        let Some((k_raw, v_raw)) = line.split_once(':') else {
            continue;
        };
        let key = k_raw.trim().to_ascii_lowercase();
        let val = v_raw.trim();
        if val.is_empty() {
            continue;
        }

        if key == "name" {
            if let Some(prev) = current.take() {
                if !prev.interface_name.is_empty() {
                    map.insert(prev.interface_name.to_ascii_lowercase(), prev);
                }
            }
            current = Some(WifiAdapterRuntimeInfo {
                interface_name: val.to_string(),
                ..Default::default()
            });
            continue;
        }

        if let Some(ref mut info) = current {
            match key.as_str() {
                "state" => {
                    info.connected = val.eq_ignore_ascii_case("connected");
                }
                "ssid" => {
                    info.ssid = Some(val.to_string());
                }
                "ap bssid" | "bssid" => {
                    info.bssid = Some(val.to_string());
                }
                "radio type" => {
                    info.radio_type = Some(val.to_string());
                }
                "signal" => {
                    let pct_str = val.trim_end_matches('%').trim();
                    if let Ok(pct) = pct_str.parse::<u8>() {
                        info.signal_pct = Some(pct.min(100));
                    }
                }
                "profile" => {
                    info.profile = Some(val.to_string());
                }
                _ => {}
            }
        }
    }

    if let Some(prev) = current.take() {
        if !prev.interface_name.is_empty() {
            map.insert(prev.interface_name.to_ascii_lowercase(), prev);
        }
    }

    map
}

/// Scans available Wi-Fi networks visible to a specific physical Wi-Fi adapter (`Wi-Fi`, `Wi-Fi 2`, etc.).
pub fn scan_wifi_networks_for_adapter(
    interface_name: &str,
) -> Result<Vec<WifiNetworkScanItem>, String> {
    let iface_arg = format!("interface={interface_name}");
    let networks_out = hidden_netsh(&["wlan", "show", "networks", &iface_arg, "mode=bssid"])
        .ok_or_else(|| "Failed to query Windows WLAN service".to_string())?;

    let profiles_out =
        hidden_netsh(&["wlan", "show", "profiles", &iface_arg]).unwrap_or_default();
    let mut saved_profiles = HashSet::new();
    for line in profiles_out.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.to_ascii_lowercase().contains("user profile") {
                let p = v.trim();
                if !p.is_empty() {
                    saved_profiles.insert(p.to_string());
                }
            }
        }
    }

    let current_if_map = query_windows_wifi_interfaces();
    let connected_ssid = current_if_map
        .get(&interface_name.to_ascii_lowercase())
        .and_then(|i| if i.connected { i.ssid.clone() } else { None });

    let mut items = parse_netsh_wlan_networks(&networks_out, &saved_profiles, connected_ssid.as_deref());
    items.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| b.signal_pct.cmp(&a.signal_pct))
            .then_with(|| a.ssid.cmp(&b.ssid))
    });
    Ok(items)
}

pub fn parse_netsh_wlan_networks(
    stdout: &str,
    saved_profiles: &HashSet<String>,
    connected_ssid: Option<&str>,
) -> Vec<WifiNetworkScanItem> {
    let mut results: Vec<WifiNetworkScanItem> = Vec::new();
    let mut curr: Option<WifiNetworkScanItem> = None;

    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        let Some((k_raw, v_raw)) = line.split_once(':') else {
            continue;
        };
        let key = k_raw.trim().to_ascii_lowercase();
        let val = v_raw.trim();

        if key.starts_with("ssid ") {
            if let Some(prev) = curr.take() {
                if !prev.ssid.is_empty() {
                    results.push(prev);
                }
            }
            let ssid = val.to_string();
            let is_conn = connected_ssid
                .map(|c| c.eq_ignore_ascii_case(&ssid))
                .unwrap_or(false);
            let has_prof = saved_profiles.iter().any(|p| p.eq_ignore_ascii_case(&ssid));
            curr = Some(WifiNetworkScanItem {
                ssid,
                bssid: String::new(),
                signal_pct: 0,
                radio_type: String::new(),
                authentication: String::new(),
                channel: String::new(),
                connected: is_conn,
                has_saved_profile: has_prof,
            });
            continue;
        }

        if let Some(ref mut item) = curr {
            if key == "authentication" {
                item.authentication = val.to_string();
            } else if key.starts_with("bssid ") && item.bssid.is_empty() {
                item.bssid = val.to_string();
            } else if key == "signal" {
                let pct = val
                    .trim_end_matches('%')
                    .trim()
                    .parse::<u8>()
                    .unwrap_or(0)
                    .min(100);
                if pct >= item.signal_pct {
                    item.signal_pct = pct;
                }
            } else if key == "radio type" && item.radio_type.is_empty() {
                item.radio_type = val.to_string();
            } else if key == "channel" && item.channel.is_empty() {
                item.channel = val.to_string();
            }
        }
    }

    if let Some(prev) = curr.take() {
        if !prev.ssid.is_empty() {
            results.push(prev);
        }
    }

    results
}

/// Connects a specific Windows Wi-Fi adapter (`Wi-Fi`, `Wi-Fi 2`, etc.) to `ssid`.
/// Supports connecting two physical Wi-Fi adapters to two different Wi-Fi routers simultaneously!
pub fn connect_wifi_adapter(
    interface_name: &str,
    ssid: &str,
    password: Option<&str>,
) -> Result<String, String> {
    if ssid.trim().is_empty() {
        return Err("SSID cannot be empty".into());
    }

    let iface_arg = format!("interface={interface_name}");

    // If a password was supplied, create & register a per-adapter WLAN XML profile
    if let Some(pass) = password.filter(|p| !p.is_empty()) {
        let xml = build_wlan_profile_xml(ssid, Some(pass));
        let temp_xml = std::env::temp_dir().join(format!(
            "netfusion_wlan_{}.xml",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&temp_xml, xml)
            .map_err(|e| format!("Failed to write temporary WLAN profile: {e}"))?;
        let file_arg = format!("filename={}", temp_xml.to_string_lossy());
        let _ = hidden_netsh(&[
            "wlan",
            "add",
            "profile",
            &file_arg,
            &iface_arg,
            "user=current",
        ]);
        let _ = std::fs::remove_file(&temp_xml);
    }

    let name_arg = format!("name={ssid}");
    let ssid_arg = format!("ssid={ssid}");
    let out = hidden_netsh(&["wlan", "connect", &name_arg, &ssid_arg, &iface_arg])
        .ok_or_else(|| "Failed to invoke netsh wlan connect".to_string())?;

    if out.to_ascii_lowercase().contains("completed successfully") {
        Ok(format!(
            "Connecting adapter '{interface_name}' to Wi-Fi network '{ssid}'"
        ))
    } else {
        Err(out.trim().to_string())
    }
}

pub fn disconnect_wifi_adapter(interface_name: &str) -> Result<(), String> {
    let iface_arg = format!("interface={interface_name}");
    let _ = hidden_netsh(&["wlan", "disconnect", &iface_arg]);
    Ok(())
}

/// Attempts an automatic non-blocking Wi-Fi reconnect using a saved Windows WLAN profile.
pub fn try_auto_reconnect_wifi(interface_name: &str, profile: &str) -> bool {
    if interface_name.is_empty() || profile.is_empty() {
        return false;
    }
    let name_arg = format!("name={profile}");
    let iface_arg = format!("interface={interface_name}");
    if let Some(out) = hidden_netsh(&["wlan", "connect", &name_arg, &iface_arg]) {
        out.to_ascii_lowercase().contains("completed successfully")
    } else {
        false
    }
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn build_wlan_profile_xml(ssid: &str, password: Option<&str>) -> String {
    let esc_ssid = xml_escape(ssid);
    let hex_ssid = hex::encode(ssid.as_bytes()).to_uppercase();

    if let Some(pass) = password.filter(|p| !p.is_empty()) {
        let esc_pass = xml_escape(pass);
        format!(
            r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>{esc_ssid}</name>
    <SSIDConfig>
        <SSID>
            <hex>{hex_ssid}</hex>
            <name>{esc_ssid}</name>
        </SSID>
    </SSIDConfig>
    <connectionType>ESS</connectionType>
    <connectionMode>auto</connectionMode>
    <MSM>
        <security>
            <authEncryption>
                <authentication>WPA2PSK</authentication>
                <encryption>AES</encryption>
                <useOneX>false</useOneX>
            </authEncryption>
            <sharedKey>
                <keyType>passPhrase</keyType>
                <protected>false</protected>
                <keyMaterial>{esc_pass}</keyMaterial>
            </sharedKey>
        </security>
    </MSM>
</WLANProfile>"#
        )
    } else {
        format!(
            r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>{esc_ssid}</name>
    <SSIDConfig>
        <SSID>
            <hex>{hex_ssid}</hex>
            <name>{esc_ssid}</name>
        </SSID>
    </SSIDConfig>
    <connectionType>ESS</connectionType>
    <connectionMode>auto</connectionMode>
    <MSM>
        <security>
            <authEncryption>
                <authentication>open</authentication>
                <encryption>none</encryption>
                <useOneX>false</useOneX>
            </authEncryption>
        </security>
    </MSM>
</WLANProfile>"#
        )
    }
}

