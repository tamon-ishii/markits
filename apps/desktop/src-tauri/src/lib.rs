pub mod capture;
pub mod commands;
pub mod history;
pub mod metadata;
pub mod ui_elements;

use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Default)]
pub struct AppState {
    pub last_capture_data_url: Mutex<Option<String>>,
}

fn trigger_capture(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("trigger-capture", ());
    }
}

fn show_editor(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
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
            commands::cmd_crop_and_load,
            commands::cmd_load_image,
            commands::cmd_render_svg,
            commands::cmd_compose_and_save,
            commands::cmd_copy_to_clipboard,
            commands::cmd_get_history,
            commands::cmd_load_history_item,
            commands::cmd_delete_history_item,
            commands::cmd_save_to_history
        ])
        .on_window_event(|window, event| {
            // Intercept window close (X button) to minimize/hide to system tray instead of exiting
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
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
            let open_item = MenuItem::with_id(app, "open", "Open Image...", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &capture_item, &open_item, &quit_item])?;

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
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
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

            // Register global shortcut for PrintScreen or Alt+PrintScreen
            let handle = app.handle().clone();
            let shortcuts = ["PrintScreen", "Alt+PrintScreen", "Control+Shift+S"];
            for key in shortcuts {
                if let Ok(shortcut) = key.parse::<Shortcut>() {
                    let h = handle.clone();
                    let _ = app.global_shortcut().on_shortcut(shortcut, move |_app, _sc, event| {
                        if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            trigger_capture(&h);
                        }
                    });
                }
            }

            // Ensure main window is shown on initial launch
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
