use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const INTERNET_SETTINGS_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

fn run_reg(args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("reg");
    cmd.args(args);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        None
    }
}

/// Safely routes Windows user-space HTTP/HTTPS/SOCKS application traffic through the
/// NetFusion Unified Virtual Bonding Gateway (`127.0.0.1:<port>`) without modifying
/// kernel routing tables or requiring Administrator privileges.
pub fn enable_windows_system_proxy(port: u16) -> Result<(), String> {
    let proxy_endpoint = format!("127.0.0.1:{port}");
    run_reg(&[
        "add",
        INTERNET_SETTINGS_KEY,
        "/v",
        "ProxyServer",
        "/t",
        "REG_SZ",
        "/d",
        &proxy_endpoint,
        "/f",
    ])
    .ok_or_else(|| "Failed to configure Windows ProxyServer registry value".to_string())?;

    let _ = run_reg(&[
        "add",
        INTERNET_SETTINGS_KEY,
        "/v",
        "ProxyOverride",
        "/t",
        "REG_SZ",
        "/d",
        "localhost;127.*;10.*;192.168.*;<local>",
        "/f",
    ]);

    run_reg(&[
        "add",
        INTERNET_SETTINGS_KEY,
        "/v",
        "ProxyEnable",
        "/t",
        "REG_DWORD",
        "/d",
        "1",
        "/f",
    ])
    .ok_or_else(|| "Failed to enable Windows System Proxy routing".to_string())?;

    Ok(())
}

/// Safely unsets the Windows user-space proxy, restoring direct OS routing.
pub fn disable_windows_system_proxy() -> Result<(), String> {
    run_reg(&[
        "add",
        INTERNET_SETTINGS_KEY,
        "/v",
        "ProxyEnable",
        "/t",
        "REG_DWORD",
        "/d",
        "0",
        "/f",
    ])
    .ok_or_else(|| "Failed to disable Windows System Proxy routing".to_string())?;
    Ok(())
}

/// Checks whether Windows System Proxy is currently enabled and pointing to `127.0.0.1:<port>`.
pub fn is_windows_system_proxy_enabled(port: u16) -> bool {
    let Some(enable_out) = run_reg(&["query", INTERNET_SETTINGS_KEY, "/v", "ProxyEnable"]) else {
        return false;
    };
    if !enable_out.contains("0x1") {
        return false;
    }
    let Some(server_out) = run_reg(&["query", INTERNET_SETTINGS_KEY, "/v", "ProxyServer"]) else {
        return false;
    };
    let expected = format!("127.0.0.1:{port}");
    server_out.contains(&expected)
}

