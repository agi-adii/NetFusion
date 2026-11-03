pub mod bonding;
pub mod commands;
pub mod db;
pub mod downloader;
pub mod logger;
pub mod models;
pub mod network;
pub mod speedtest;

use bonding::client::BondingTunnelClient;
use db::Database;
use downloader::engine::DownloadEngine;
use logger::AppLogger;
use network::monitor::NetworkMonitor;
use speedtest::SpeedTestEngine;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;

pub struct AppState {
    pub db: Arc<Database>,
    pub logger: AppLogger,
    pub bonding_client: Arc<BondingTunnelClient>,
    pub monitor: Arc<NetworkMonitor>,
    pub downloader: Arc<DownloadEngine>,
    pub speedtest: Arc<SpeedTestEngine>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("NetFusion");
    let db_path = data_dir.join("netfusion.db");

    let db = Arc::new(Database::open(db_path).expect("Failed to initialize NetFusion SQLite database"));
    let logger = AppLogger::new(db.clone());
    let bonding_client = BondingTunnelClient::new(db.clone(), logger.clone());
    let monitor = NetworkMonitor::new(db.clone(), logger.clone(), bonding_client.clone());
    let downloader = DownloadEngine::new(
        db.clone(),
        logger.clone(),
        monitor.clone(),
        bonding_client.clone(),
    );
    let speedtest = SpeedTestEngine::new(
        db.clone(),
        logger.clone(),
        monitor.clone(),
        bonding_client.clone(),
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(AppState {
            db: db.clone(),
            logger: logger.clone(),
            bonding_client: bonding_client.clone(),
            monitor: monitor.clone(),
            downloader: downloader.clone(),
            speedtest: speedtest.clone(),
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            logger.attach_app_handle(handle.clone());
            downloader.attach_app_handle(handle.clone());
            speedtest.attach_app_handle(handle.clone());

            logger.info(
                "SYSTEM",
                "NetFusion Multipath Bonding & Download Engine initialized",
            );

            // Build Windows System Tray Menu (Show Window, Pause All, Resume All, Quit)
            let show_i = MenuItem::with_id(app, "show", "Open NetFusion", true, None::<&str>)?;
            let pause_i =
                MenuItem::with_id(app, "pause_all", "Pause All Downloads", true, None::<&str>)?;
            let resume_i =
                MenuItem::with_id(app, "resume_all", "Resume All Downloads", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Exit NetFusion", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &pause_i, &resume_i, &quit_i])?;

            let dl_for_menu = downloader.clone();
            let mut tray_builder = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("NetFusion — Internet Bonding & Download Manager")
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "pause_all" => {
                        let d = dl_for_menu.clone();
                        tauri::async_runtime::spawn(async move {
                            d.pause_all().await;
                        });
                    }
                    "resume_all" => {
                        let d = dl_for_menu.clone();
                        tauri::async_runtime::spawn(async move {
                            d.resume_all().await;
                        });
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                });

            if let Some(default_icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(default_icon);
            }
            let _tray = tray_builder.build(app)?;

            // Start 1 Hz real Windows adapter telemetry loop + download queue scheduler
            let mon_start = monitor.clone();
            let dl_start = downloader.clone();
            let bond_start = bonding_client.clone();
            let db_start = db.clone();
            let h_clone = handle.clone();

            tauri::async_runtime::spawn(async move {
                let settings = db_start.get_settings();
                if settings.enable_local_socks_proxy {
                    let _ = bond_start
                        .ensure_local_socks_listener(settings.local_socks_port)
                        .await;
                }
                let _ = mon_start.sample_once(&Some(h_clone.clone())).await;
                mon_start.start_background_loop(Some(h_clone));
                dl_start.start_scheduler_loop();
            });

            // Handle --minimized startup flag
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let settings = state.db.get_settings();
                if settings.minimize_to_tray && settings.run_in_background {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_network_telemetry,
            commands::set_interface_enabled,
            commands::set_interface_custom_weight,
            commands::set_primary_failover_interface,
            commands::set_bonding_mode,
            commands::connect_bonding_server,
            commands::disconnect_bonding_server,
            commands::get_connection_history,
            commands::list_downloads,
            commands::probe_download_url,
            commands::add_download,
            commands::pause_download,
            commands::resume_download,
            commands::retry_download,
            commands::cancel_download,
            commands::update_download_priority,
            commands::update_download_speed_limit,
            commands::pause_all_downloads,
            commands::resume_all_downloads,
            commands::clear_completed_downloads,
            commands::open_downloaded_file,
            commands::open_download_folder,
            commands::run_speed_test,
            commands::cancel_speed_test,
            commands::get_speed_test_history,
            commands::get_settings,
            commands::update_settings,
            commands::get_logs,
            commands::clear_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NetFusion application");
}

