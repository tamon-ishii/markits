pub mod capture;
pub mod commands;
pub mod history;
pub mod metadata;
pub mod ui_elements;
pub mod recording;
pub mod playback;
mod cursor_capture;

use std::sync::{Mutex, atomic::AtomicU64};
use tauri::{
    AppHandle, Emitter, Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Default)]
pub struct AppState {
    pub cursor_target: Mutex<Option<cursor_capture::CursorCaptureTarget>>,
    pub cursor_guide: Mutex<Option<cursor_capture::CursorGuideStatus>>,
    pub cursor_capture_busy: std::sync::atomic::AtomicBool,
    pub last_capture_data_url: Mutex<Option<String>>,
    pub overlay_ready: Mutex<bool>,
    pub pending_capture: Mutex<Option<capture::CapturedImage>>,
    pub capture_generation: AtomicU64,
    pub shortcut: Mutex<String>,
    pub shortcut_warning: Mutex<Option<String>>,
    pub manual_studio: Mutex<Option<ManualStudioSession>>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualStudioSession { pub input: String, pub output: String, pub completion: String }

#[tauri::command]
fn cmd_manual_studio_session(app: AppHandle) -> Result<Option<ManualStudioSession>, String> {
    Ok(app.state::<AppState>().manual_studio.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
fn cmd_manual_studio_complete(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let session = state.manual_studio.lock().map_err(|e| e.to_string())?.clone().ok_or("Manual Studio連携モードではありません")?;
    std::fs::write(session.completion, "saved").map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

const CAPTURE_SHORTCUTS: &[&str] = &["PrintScreen", "Alt+PrintScreen", "Control+Shift+S"];

fn shortcut_config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("markits")
        .join("capture-shortcut.txt")
}

fn exploration_depth_config_path() -> std::path::PathBuf {
    shortcut_config_path().with_file_name("ui-exploration-depth.txt")
}
fn read_exploration_depth() -> usize {
    std::fs::read_to_string(exploration_depth_config_path()).ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|depth| ui_elements::validate_exploration_depth(*depth).is_ok())
        .unwrap_or(ui_elements::DEFAULT_EXPLORATION_DEPTH)
}
#[tauri::command]
fn cmd_get_ui_exploration_depth() -> usize { ui_elements::exploration_depth() }
#[tauri::command]
fn cmd_set_ui_exploration_depth(depth: usize) -> Result<usize, String> {
    ui_elements::validate_exploration_depth(depth)?;
    let path = exploration_depth_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, depth.to_string())
        .map_err(|error| format!("UI探索階層の設定を保存できません: {error}"))?;
    ui_elements::set_exploration_depth(depth)?;
    Ok(depth)
}

fn read_shortcut() -> String {
    std::fs::read_to_string(shortcut_config_path())
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| CAPTURE_SHORTCUTS.contains(&value.as_str()))
        .unwrap_or_else(|| CAPTURE_SHORTCUTS[0].to_string())
}

#[derive(serde::Serialize)]
struct ShortcutStatus {
    shortcut: String,
    warning: Option<String>,
}

#[tauri::command]
fn cmd_get_capture_shortcut(app: AppHandle) -> Result<ShortcutStatus, String> {
    let state = app.state::<AppState>();
    Ok(ShortcutStatus {
        shortcut: state.shortcut.lock().map_err(|e| e.to_string())?.clone(),
        warning: state
            .shortcut_warning
            .lock()
            .map_err(|e| e.to_string())?
            .clone(),
    })
}

#[tauri::command]
fn cmd_set_capture_shortcut(app: AppHandle, shortcut: String) -> Result<ShortcutStatus, String> {
    if !CAPTURE_SHORTCUTS.contains(&shortcut.as_str()) {
        return Err("Unsupported capture shortcut".to_string());
    }
    let state = app.state::<AppState>();
    let previous = state.shortcut.lock().map_err(|e| e.to_string())?.clone();
    if previous != shortcut {
        if let Ok(old) = previous.parse::<Shortcut>() {
            let _ = app.global_shortcut().unregister(old);
        }
        *state.shortcut.lock().map_err(|e| e.to_string())? = shortcut.clone();
        if let Err(error) = register_capture_shortcuts(&app) {
            *state.shortcut.lock().map_err(|e| e.to_string())? = previous;
            let _ = register_capture_shortcuts(&app);
            return Err(error);
        }
        let path = shortcut_config_path();
        let persist = (|| -> Result<(), std::io::Error> {
            if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
            std::fs::write(path, &shortcut)
        })();
        if let Err(error) = persist {
            if let Ok(new_shortcut) = shortcut.parse::<Shortcut>() {
                let _ = app.global_shortcut().unregister(new_shortcut);
            }
            *state.shortcut.lock().map_err(|e| e.to_string())? = previous;
            let _ = register_capture_shortcuts(&app);
            return Err(format!("ショートカット設定を保存できません: {error}"));
        }
    }
    cmd_get_capture_shortcut(app)
}

fn trigger_capture(app: &AppHandle) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = if app_handle.state::<recording::RecordingState>().is_active() {
            recording::cmd_recording_screenshot(app_handle.clone(), app_handle.state()).await.map(|_| ())
        } else {
            match cursor_capture::capture_pending(&app_handle).await {
                Ok(true) => Ok(()),
                Ok(false) => commands::cmd_start_capture(app_handle.clone()).await,
                Err(error) => Err(error),
            }
        };
        if let Err(err) = result {
            if app_handle.state::<recording::RecordingState>().is_active() {
                let _ = app_handle.emit_to("recording", "recording-capture-error", err);
                return;
            }
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
        let _ = register_capture_shortcuts(app);
    }
}

fn register_capture_shortcuts(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let key = state.shortcut.lock().map_err(|e| e.to_string())?.clone();
    let shortcut = key.parse::<Shortcut>().map_err(|e| e.to_string())?;
    if app.global_shortcut().is_registered(shortcut) {
        return Ok(());
    }
    let handle = app.clone();
    let result = app
        .global_shortcut()
        .on_shortcut(shortcut, move |_app, _sc, event| {
            if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                trigger_capture(&handle);
            }
        })
        .map_err(|e| format!("ショートカット {key} を登録できません: {e}"));
    *state.shortcut_warning.lock().map_err(|e| e.to_string())? = result.as_ref().err().cloned();
    result
}

pub fn run() {
    let _ = ui_elements::set_exploration_depth(read_exploration_depth());
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--manual-studio-record-input") {
        let result = args.get(1).ok_or_else(|| "Recorder output path is required".to_string())
            .and_then(|path| markits::recording::run_helper(std::path::Path::new(path)));
        if let Err(error) = result { eprintln!("{error}"); std::process::exit(2); }
        return;
    }
    let input = args.windows(2).find(|pair| pair[0] == "--manual-studio-input").map(|pair| pair[1].clone());
    let output = args.windows(2).find(|pair| pair[0] == "--manual-studio-output").map(|pair| pair[1].clone());
    let completion = args.windows(2).find(|pair| pair[0] == "--manual-studio-completion").map(|pair| pair[1].clone());
    let manual_studio = input.zip(output).zip(completion).map(|((input, output), completion)| ManualStudioSession { input, output, completion });
    let background = args.iter().any(|arg| arg == "--background") && manual_studio.is_none();
    tauri::Builder::default()
        .manage(recording::RecordingState::default())
        .manage(playback::PlaybackState::default())
        .manage(AppState {
            shortcut: Mutex::new(read_shortcut()),
            manual_studio: Mutex::new(manual_studio),
            ..AppState::default()
        })
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            playback::cmd_has_operations,
            playback::cmd_playback,
            playback::cmd_playback_cancel,
            playback::cmd_playback_control,
            playback::cmd_playback_capture_wait,
            playback::cmd_playback_status,
            playback::cmd_playback_ready,
            recording::cmd_recording_screenshot,
            recording::cmd_recording_cursor_mode,
            recording::cmd_recording_windows,
            recording::cmd_recording_start,
            recording::cmd_recording_start_from_image,
            recording::cmd_recording_status,
            recording::cmd_recording_pause,
            recording::cmd_recording_finish,
            recording::cmd_recording_cancel,
            commands::cmd_capture_screen,
            commands::cmd_start_capture,
            commands::cmd_select_recording_window,
            cursor_capture::cmd_cursor_capture_supported,
            cursor_capture::cmd_arm_cursor_capture,
            cursor_capture::cmd_cursor_guide_status,
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
            commands::cmd_fetch_detailed_ui_elements,
            cmd_get_ui_exploration_depth,
            cmd_set_ui_exploration_depth,
            cmd_get_capture_shortcut,
            cmd_set_capture_shortcut,
            cmd_manual_studio_session,
            cmd_manual_studio_complete
        ])
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "cursor-guide" {
                    api.prevent_close();
                    let _ = commands::cmd_cancel_capture(window.app_handle().clone());
                    return;
                }
                if window.label() == "playback" {
                    window.state::<playback::PlaybackState>().1.store(true, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
                if window.label() == "recording" {
                    api.prevent_close();
                    recording::cancel(window.app_handle());
                    return;
                }
                if window.label() == "main" {
                    let session = window.state::<AppState>().manual_studio.lock().ok().and_then(|session| session.clone());
                    if let Some(session) = session {
                        let marker = std::path::Path::new(&session.completion).with_extension("exit");
                        let _ = std::fs::write(marker, "MarkItsの編集を中止しました。元画像は保持しています。再度編集できます。");
                        window.app_handle().exit(0);
                        return;
                    }
                }
                api.prevent_close();
                if window.label() == "overlay" {
                    let _ = commands::cmd_cancel_capture(window.app_handle().clone());
                } else {
                    let _ = window.hide();
                }
            }
            tauri::WindowEvent::Focused(true) if window.label() == "main" => {
                let _ = register_capture_shortcuts(window.app_handle());
            }
            _ => {}
        })
        .setup(move |app| {
            // Build system tray menu
            let show_item = MenuItem::with_id(app, "show", "Show Editor", true, None::<&str>)?;
            let capture_item = MenuItem::with_id(
                app,
                "capture",
                "Capture Screen",
                true,
                None::<&str>,
            )?;
            let cancel_item =
                MenuItem::with_id(app, "cancel", "Cancel Capture", true, None::<&str>)?;
            let open_item = MenuItem::with_id(app, "open", "Open Image...", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &show_item,
                    &capture_item,
                    &cancel_item,
                    &open_item,
                    &quit_item,
                ],
            )?;

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
                        recording::cancel(app);
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

            let _ = register_capture_shortcuts(app.handle());

            // Login startup stays in the tray; ordinary launches open the editor.
            if !background && let Some(window) = app.get_webview_window("main") {
                commands::force_raise_window(&window);
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) { recording::shutdown(app); }
        });
}
