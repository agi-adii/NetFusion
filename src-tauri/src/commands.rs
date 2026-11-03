use crate::AppState;
use crate::models::{
    AddDownloadRequest, AppSettings, ConnectionHistoryEvent, DownloadItem, DownloadPriority,
    LogEntry, NetworkTelemetrySnapshot, SpeedTestRecord, UpdateSettingsPayload, UrlProbeResult,
};
use netfusion_protocol::scheduler::BondingPolicy;
use std::path::Path;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn get_network_telemetry(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<NetworkTelemetrySnapshot, String> {
    let current = state.monitor.get_snapshot().await;
    if current.interfaces.is_empty() {
        Ok(state.monitor.sample_once(&Some(app)).await)
    } else {
        Ok(current)
    }
}

#[tauri::command]
pub async fn set_interface_enabled(
    state: State<'_, AppState>,
    app: AppHandle,
    interface_id: String,
    enabled: bool,
) -> Result<(), String> {
    let prefs = state.db.load_interface_prefs();
    let (_, weight, primary) = prefs.get(&interface_id).copied().unwrap_or((true, 50, false));
    state
        .db
        .upsert_interface_pref(&interface_id, enabled, weight, primary);
    state.logger.info(
        "BONDING",
        format!(
            "Interface {} {}",
            interface_id,
            if enabled {
                "enabled for multipath bonding"
            } else {
                "disabled from multipath bonding"
            }
        ),
    );
    let _ = state.monitor.sample_once(&Some(app)).await;
    Ok(())
}

#[tauri::command]
pub async fn set_interface_custom_weight(
    state: State<'_, AppState>,
    app: AppHandle,
    interface_id: String,
    weight: u32,
) -> Result<(), String> {
    let prefs = state.db.load_interface_prefs();
    let (enabled, _, primary) = prefs.get(&interface_id).copied().unwrap_or((true, 50, false));
    state
        .db
        .upsert_interface_pref(&interface_id, enabled, weight.clamp(1, 100), primary);
    let _ = state.monitor.sample_once(&Some(app)).await;
    Ok(())
}

#[tauri::command]
pub async fn set_primary_failover_interface(
    state: State<'_, AppState>,
    app: AppHandle,
    interface_id: String,
) -> Result<(), String> {
    let snap = state.monitor.get_snapshot().await;
    let prefs = state.db.load_interface_prefs();
    for nic in &snap.interfaces {
        let (en, w, _) = prefs.get(&nic.id).copied().unwrap_or((true, 50, false));
        state
            .db
            .upsert_interface_pref(&nic.id, en, w, nic.id == interface_id);
    }
    state.logger.info(
        "BONDING",
        format!("Updated primary failover interface to {}", interface_id),
    );
    let _ = state.monitor.sample_once(&Some(app)).await;
    Ok(())
}

#[tauri::command]
pub async fn set_bonding_mode(
    state: State<'_, AppState>,
    app: AppHandle,
    mode: BondingPolicy,
) -> Result<(), String> {
    state.db.set_bonding_mode_only(mode);
    state
        .logger
        .info("BONDING", format!("Switched bonding policy to {:?}", mode));
    let _ = state.monitor.sample_once(&Some(app)).await;
    Ok(())
}

#[tauri::command]
pub async fn connect_bonding_server(
    state: State<'_, AppState>,
    app: AppHandle,
    endpoint: String,
    auth_token: Option<String>,
) -> Result<(), String> {
    let mut settings = state.db.get_settings();
    settings.bonding_server_endpoint = endpoint.clone();
    settings.bonding_tunnel_enabled = true;
    let _ = state.db.save_settings(&UpdateSettingsPayload {
        default_download_dir: settings.default_download_dir,
        max_concurrent_downloads: settings.max_concurrent_downloads,
        default_segments_per_download: settings.default_segments_per_download,
        adaptive_concurrency: settings.adaptive_concurrency,
        global_speed_limit_bps: settings.global_speed_limit_bps,
        auto_retry_downloads: settings.auto_retry_downloads,
        max_retries: settings.max_retries,
        bonding_mode: settings.bonding_mode,
        bonding_tunnel_enabled: true,
        bonding_server_endpoint: endpoint.clone(),
        new_bonding_auth_token: auth_token.clone(),
        enable_local_socks_proxy: settings.enable_local_socks_proxy,
        local_socks_port: settings.local_socks_port,
        launch_at_startup: settings.launch_at_startup,
        minimize_to_tray: settings.minimize_to_tray,
        run_in_background: settings.run_in_background,
        notify_on_complete: settings.notify_on_complete,
        theme: settings.theme,
        speedtest_endpoint: settings.speedtest_endpoint,
    });

    let snap = state.monitor.get_snapshot().await;
    let res = state
        .bonding_client
        .connect_server(endpoint, auth_token, &snap.interfaces)
        .await;
    let _ = state.monitor.sample_once(&Some(app)).await;
    res
}

#[tauri::command]
pub async fn disconnect_bonding_server(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    state.bonding_client.disconnect_server().await;
    let settings = state.db.get_settings();
    let _ = state.db.save_settings(&UpdateSettingsPayload {
        default_download_dir: settings.default_download_dir,
        max_concurrent_downloads: settings.max_concurrent_downloads,
        default_segments_per_download: settings.default_segments_per_download,
        adaptive_concurrency: settings.adaptive_concurrency,
        global_speed_limit_bps: settings.global_speed_limit_bps,
        auto_retry_downloads: settings.auto_retry_downloads,
        max_retries: settings.max_retries,
        bonding_mode: settings.bonding_mode,
        bonding_tunnel_enabled: false,
        bonding_server_endpoint: settings.bonding_server_endpoint,
        new_bonding_auth_token: None,
        enable_local_socks_proxy: settings.enable_local_socks_proxy,
        local_socks_port: settings.local_socks_port,
        launch_at_startup: settings.launch_at_startup,
        minimize_to_tray: settings.minimize_to_tray,
        run_in_background: settings.run_in_background,
        notify_on_complete: settings.notify_on_complete,
        theme: settings.theme,
        speedtest_endpoint: settings.speedtest_endpoint,
    });
    let _ = state.monitor.sample_once(&Some(app)).await;
    Ok(())
}

#[tauri::command]
pub async fn get_connection_history(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionHistoryEvent>, String> {
    Ok(state.db.list_connection_events(200))
}

#[tauri::command]
pub async fn list_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadItem>, String> {
    Ok(state.downloader.list_downloads().await)
}

#[tauri::command]
pub async fn probe_download_url(
    state: State<'_, AppState>,
    url: String,
) -> Result<UrlProbeResult, String> {
    state.downloader.probe_url(&url).await
}

#[tauri::command]
pub async fn add_download(
    state: State<'_, AppState>,
    request: AddDownloadRequest,
) -> Result<DownloadItem, String> {
    state.downloader.add_download(request).await
}

#[tauri::command]
pub async fn pause_download(
    state: State<'_, AppState>,
    download_id: String,
) -> Result<(), String> {
    state.downloader.pause_download(&download_id).await
}

#[tauri::command]
pub async fn resume_download(
    state: State<'_, AppState>,
    download_id: String,
) -> Result<(), String> {
    state.downloader.resume_download(&download_id).await
}

#[tauri::command]
pub async fn retry_download(
    state: State<'_, AppState>,
    download_id: String,
) -> Result<(), String> {
    state.downloader.retry_download(&download_id).await
}

#[tauri::command]
pub async fn cancel_download(
    state: State<'_, AppState>,
    download_id: String,
    delete_files: bool,
) -> Result<(), String> {
    state
        .downloader
        .cancel_download(&download_id, delete_files)
        .await
}

#[tauri::command]
pub async fn update_download_priority(
    state: State<'_, AppState>,
    download_id: String,
    priority: DownloadPriority,
) -> Result<(), String> {
    state
        .downloader
        .update_priority(&download_id, priority)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn update_download_speed_limit(
    state: State<'_, AppState>,
    download_id: String,
    speed_limit_bps: Option<u64>,
) -> Result<(), String> {
    state
        .downloader
        .update_speed_limit(&download_id, speed_limit_bps)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn pause_all_downloads(state: State<'_, AppState>) -> Result<(), String> {
    state.downloader.pause_all().await;
    Ok(())
}

#[tauri::command]
pub async fn resume_all_downloads(state: State<'_, AppState>) -> Result<(), String> {
    state.downloader.resume_all().await;
    Ok(())
}

#[tauri::command]
pub async fn clear_completed_downloads(state: State<'_, AppState>) -> Result<(), String> {
    state.downloader.clear_completed().await;
    Ok(())
}

#[tauri::command]
pub async fn open_downloaded_file(
    state: State<'_, AppState>,
    app: AppHandle,
    download_id: String,
) -> Result<(), String> {
    let items = state.downloader.list_downloads().await;
    let item = items
        .iter()
        .find(|d| d.id == download_id)
        .ok_or("Download not found")?;
    if !Path::new(&item.target_path).exists() {
        return Err("File does not exist on disk yet".into());
    }
    app.opener()
        .open_path(&item.target_path, None::<&str>)
        .map_err(|e| format!("Failed to open file: {e}"))
}

#[tauri::command]
pub async fn open_download_folder(
    state: State<'_, AppState>,
    app: AppHandle,
    download_id: String,
) -> Result<(), String> {
    let items = state.downloader.list_downloads().await;
    let item = items
        .iter()
        .find(|d| d.id == download_id)
        .ok_or("Download not found")?;

    if Path::new(&item.target_path).exists() {
        let _ = std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", item.target_path))
            .spawn();
        return Ok(());
    }

    app.opener()
        .open_path(&item.save_dir, None::<&str>)
        .map_err(|e| format!("Failed to open folder: {e}"))
}

#[tauri::command]
pub async fn run_speed_test(
    state: State<'_, AppState>,
    interface_id: Option<String>,
) -> Result<SpeedTestRecord, String> {
    state.speedtest.run_test(interface_id).await
}

#[tauri::command]
pub async fn cancel_speed_test(state: State<'_, AppState>) -> Result<(), String> {
    state.speedtest.cancel();
    Ok(())
}

#[tauri::command]
pub async fn get_speed_test_history(
    state: State<'_, AppState>,
) -> Result<Vec<SpeedTestRecord>, String> {
    Ok(state.db.list_speed_tests(100))
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.db.get_settings())
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    app: AppHandle,
    payload: UpdateSettingsPayload,
) -> Result<AppSettings, String> {
    let saved = state.db.save_settings(&payload)?;

    // Sync Windows Startup autostart setting
    let autostart = app.autolaunch();
    if saved.launch_at_startup {
        let _ = autostart.enable();
    } else {
        let _ = autostart.disable();
    }

    state
        .logger
        .info("SYSTEM", "Updated application settings and preferences");
    Ok(saved)
}

#[tauri::command]
pub async fn get_logs(state: State<'_, AppState>) -> Result<Vec<LogEntry>, String> {
    Ok(state.db.list_logs(500))
}

#[tauri::command]
pub async fn clear_logs(state: State<'_, AppState>) -> Result<(), String> {
    state.db.clear_logs();
    Ok(())
}

