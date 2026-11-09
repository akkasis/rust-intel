//! Rust Intelligence Desktop — локальное приложение для анализа публичной игровой активности Rust.

pub mod commands;
pub mod config;
pub mod domain;
pub mod error;
mod fixtures_export;
pub mod infrastructure;
pub mod persistence;
pub mod providers;
pub mod rustplus;
pub mod services;
pub mod state;
pub mod tray;

use infrastructure::secrets::SecretStore;
use state::AppState;
use std::sync::Arc;
use tauri::{Manager, WindowEvent};

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,rid_lib=info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_target(false).try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let state = Arc::new(AppState::new(data_dir, Arc::new(SecretStore::new())));
            let cfg = tauri::async_runtime::block_on(state.config());
            tray::set_close_to_tray(cfg.settings.close_to_tray);
            app.manage(state.clone());
            tray::setup(app.handle(), cfg.settings.tracking_paused)?;
            if std::env::args().any(|a| a == "--minimized")
                && let Some(w) = app.get_webview_window("main")
            {
                let _ = w.hide();
            }
            let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
            tauri::async_runtime::spawn(rustplus::service::start(app.handle().clone(), state.clone()));
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(services::tracker_service::run_loop(state, move |ctx, ids| {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move { commands::dispatch_events(&h, ctx, ids).await });
            }));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
                && tray::CLOSE_TO_TRAY.load(std::sync::atomic::Ordering::Relaxed)
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::choose_mode,
            commands::update_settings,
            commands::parse_identifier,
            commands::search_player,
            commands::recent_searches,
            commands::list_players,
            commands::get_player_profile,
            commands::refresh_player,
            commands::search_battlemetrics,
            commands::link_battlemetrics,
            commands::unlink_battlemetrics,
            commands::get_bm_player,
            commands::get_bm_activity,
            commands::list_bm_players,
            commands::set_bm_tracked,
            commands::link_bm_to_steam,
            commands::get_server_online,
            commands::set_server_watch,
            commands::list_server_watch,
            commands::get_bm_routine,
            commands::get_bm_post_wipe,
            commands::get_bm_alts,
            commands::list_bm_clans,
            commands::save_bm_clan,
            commands::delete_bm_clan,
            commands::add_bm_clan_member,
            commands::remove_bm_clan_member,
            commands::bm_clan_from_teammates,
            commands::get_bm_clan_report,
            commands::get_server_insights,
            commands::get_bm_link_candidates,
            commands::get_steam_friends,
            commands::rustplus_status,
            commands::rustplus_login,
            commands::rustplus_logout,
            commands::rustplus_update_server,
            commands::rustplus_remove_server,
            commands::rustplus_test_message,
            commands::rustplus_self_test,
            commands::rustplus_reregister,
            commands::set_favorite,
            commands::set_player_notes,
            commands::delete_player,
            commands::purge_player_history,
            commands::get_activity,
            commands::list_squads,
            commands::save_squad,
            commands::delete_squad,
            commands::add_squad_member,
            commands::remove_squad_member,
            commands::get_squad_report,
            commands::list_tracked,
            commands::set_tracking,
            commands::remove_tracking,
            commands::set_tracking_notify,
            commands::set_tracking_paused,
            commands::get_tracker_status,
            commands::poll_now,
            commands::list_events,
            commands::mark_events_read,
            commands::get_bans_overview,
            commands::list_servers,
            commands::refresh_servers,
            commands::set_server_followed,
            commands::get_server_population,
            commands::get_integrations,
            commands::set_secret,
            commands::clear_secret,
            commands::test_integration,
            commands::send_test_notification,
            commands::export_data,
            commands::import_data,
            commands::backup_database,
            commands::restore_database,
            commands::get_diagnostics,
            commands::purge_all_data,
            commands::get_dashboard,
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска Rust Intelligence Desktop");
}
