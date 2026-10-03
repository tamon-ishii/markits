pub mod capture;
pub mod commands;
pub mod history;
pub mod metadata;
pub mod ui_elements;

use std::sync::{atomic::AtomicU64, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Default)]
pub struct AppState {
    pub last_capture_data_url: Mutex<Option<String>>,
    pub overlay_ready: Mutex<bool>,
    pub pending_capture: Mutex<Option<capture::CapturedImage>>,
    pub capture_generation: AtomicU64,
}

fn trigger_capture(app: &AppHandle) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = commands::cmd_start_capture(app_handle.clone()).await {
            if let Some(window) = app_handle.get_webview_window("main") {
                commands::force_raise_window(&window);
                let _ = window.emit("capture-error", err);
            }
        }
    });
}

fn show_editor(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        commands::force_raise_window(&window);
        register_capture_shortcuts(app);
    }
}

fn register_capture_shortcuts(app: &AppHandle) {
    for key in ["PrintScreen", "Alt+PrintScreen", "Control+Shift+S"] {
        if let Ok(shortcut) = key.parse::<Shortcut>() {
            if app.global_shortcut().is_registered(shortcut) {
                continue;
            }
            let handle = app.clone();
            let _ = app.global_shortcut().on_shortcut(shortcut, move |_app, _sc, event| {
                if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                    trigger_capture(&handle);
                }
            });
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::cmd_capture_screen,
            commands::cmd_start_capture,
            commands::cmd_overlay_ready,
            commands::cmd_finish_capture,
            commands::cmd_cancel_capture,
            commands::cmd_crop_and_load,
            commands::cmd_load_image,
            commands::cmd_render_svg,
            commands::cmd_compose_and_save,
            commands::cmd_copy_to_clipboard,
            commands::cmd_get_history,
            commands::cmd_load_history_item,
            commands::cmd_delete_history_item,
            commands::cmd_save_to_history,
            commands::cmd_raise_window,
            commands::cmd_fetch_detailed_ui_elements
        ])
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    if window.label() == "overlay" {
                        let _ = commands::cmd_cancel_capture(window.app_handle().clone());
                    } else {
                        let _ = window.app_handle().global_shortcut().unregister_all();
                        let _ = window.hide();
                    }
                }
                tauri::WindowEvent::Focused(true) if window.label() == "main" => {
                    register_capture_shortcuts(window.app_handle());
                }
                _ => {}
            }
        })
        .setup(|app| {
            // Build system tray menu
            let show_item = MenuItem::with_id(app, "show", "Show Editor", true, None::<&str>)?;
            let capture_item = MenuItem::with_id(
                app,
                "capture",
                "Capture Screen (PrintScreen)",
                true,
                None::<&str>,
            )?;
            let cancel_item = MenuItem::with_id(app, "cancel", "Cancel Capture", true, None::<&str>)?;
            let open_item = MenuItem::with_id(app, "open", "Open Image...", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &capture_item, &cancel_item, &open_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("MarkIts Screen Capture (Running in Tray)")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        show_editor(app);
                    }
                    "capture" => {
                        trigger_capture(app);
                    }
                    "cancel" => {
                        let _ = commands::cmd_cancel_capture(app.clone());
                    }
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            show_editor(app);
                            let _ = window.emit("trigger-open-file", ());
                        }
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
                        show_editor(tray.app_handle());
                    }
                })
                .build(app)?;

            register_capture_shortcuts(app.handle());

            // Ensure main window is shown and raised on initial launch
            if let Some(window) = app.get_webview_window("main") {
                commands::force_raise_window(&window);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
