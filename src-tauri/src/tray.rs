//! Системный трей: открыть, пауза/возобновление трекера, выход.

use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

pub static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(false);

pub struct TrayItems {
    pub pause: MenuItem<Wry>,
}

fn pause_label(paused: bool) -> &'static str {
    if paused { "Возобновить трекер" } else { "Приостановить трекер" }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn setup(app: &AppHandle, paused: bool) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Открыть RID", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", pause_label(paused), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &quit])?;
    app.manage(TrayItems { pause });
    let mut b = TrayIconBuilder::with_id("rid-tray").tooltip("Rust Intelligence Desktop").menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.on_menu_event(|app, event| match event.id.as_ref() {
        "open" => show_main(app),
        "pause" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<Arc<AppState>>();
                let mut s = state.config().await.settings;
                s.tracking_paused = !s.tracking_paused;
                let paused = s.tracking_paused;
                if state.update_settings(s).await.is_ok() {
                    refresh_menu(&app, paused);
                    use tauri::Emitter;
                    let _ = app.emit("rid://settings", ());
                }
            });
        }
        "quit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            show_main(tray.app_handle());
        }
    })
    .build(app)?;
    Ok(())
}

pub fn refresh_menu(app: &AppHandle, paused: bool) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.pause.set_text(pause_label(paused));
    }
    let _ = paused;
}

pub fn set_close_to_tray(v: bool) {
    CLOSE_TO_TRAY.store(v, Ordering::Relaxed);
}
