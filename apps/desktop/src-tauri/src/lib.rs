pub mod capture;
pub mod commands;
mod image_export;
pub mod history;
pub mod metadata;
pub mod ui_elements;
pub mod recording;
pub mod playback;
mod i18n;
pub mod playback_trace;
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
    pub launch_capture_options: LaunchCaptureOptions,
    pub cursor_target: Mutex<Option<cursor_capture::CursorCaptureTarget>>,
    pub cursor_guide: Mutex<Option<cursor_capture::CursorGuideStatus>>,
    pub cursor_capture_busy: std::sync::atomic::AtomicBool,
    pub capture_shortcut_down: std::sync::atomic::AtomicBool,
    pub last_capture_data_url: Mutex<Option<String>>,
    pub overlay_ready: Mutex<bool>,
    pub pending_capture: Mutex<Option<capture::CapturedImage>>,
    pub capture_generation: AtomicU64,
    pub capture_ready_for_editing: std::sync::atomic::AtomicBool,
    pub shortcut: Mutex<String>,
    pub shortcut_warning: Mutex<Option<String>>,
    pub manual_studio: Mutex<Option<ManualStudioSession>>,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct LaunchCaptureOptions {
    pub include_cursor: bool,
    pub shortcut: Option<String>,
}

impl LaunchCaptureOptions {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--include-cursor" => options.include_cursor = true,
                "--no-cursor" => options.include_cursor = false,
                "--capture-shortcut" => {
                    let key = args.next().ok_or("--capture-shortcut requires a key combination")?;
                    if !valid_capture_shortcut(key) { return Err(format!("Invalid capture shortcut: {key}")); }
                    options.shortcut = Some(key.clone());
                }
                _ => {}
            }
        }
        Ok(options)
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualStudioSession { pub input: String, pub output: String, pub completion: String }

impl AppState {
    fn accept_capture_shortcut(&self, pressed: bool) -> bool {
        let was_down = self.capture_shortcut_down.swap(pressed, std::sync::atomic::Ordering::SeqCst);
        pressed && !was_down
    }
    pub(crate) fn exit_on_capture_cancel(&self) -> Result<bool, String> {
        let capture_handoff = self.manual_studio.lock().map_err(|e| e.to_string())?
            .as_ref().is_some_and(|session| session.input.is_empty());
        Ok(capture_handoff && !self.capture_ready_for_editing.load(std::sync::atomic::Ordering::SeqCst))
    }
}

fn launch_session(args: &[String]) -> Option<ManualStudioSession> {
    let input = args.windows(2).find(|pair| pair[0] == "--manual-studio-input").map(|pair| pair[1].clone());
    let output = args.windows(2).find(|pair| pair[0] == "--manual-studio-output").map(|pair| pair[1].clone());
    let completion = args.windows(2).find(|pair| pair[0] == "--manual-studio-completion").map(|pair| pair[1].clone());
    let input = input.or_else(|| args.iter().any(|arg| arg == "--capture").then(String::new));
    input.zip(output).zip(completion).map(|((input, output), completion)| ManualStudioSession { input, output, completion })
}

struct StartupCapture(std::sync::atomic::AtomicBool);

impl StartupCapture {
    fn take(&self) -> bool {
        self.0.swap(false, std::sync::atomic::Ordering::SeqCst)
    }
}

#[tauri::command]
fn cmd_take_startup_capture(state: tauri::State<'_, StartupCapture>) -> bool {
    state.take()
}

#[cfg(test)]
mod startup_capture_tests {
    use super::StartupCapture;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn launch_capture_options_reach_selection_without_losing_cursor_setting() {
        let args = ["--capture", "--include-cursor", "--capture-shortcut", "Control+Shift+E",
            "--manual-studio-output", "out.png", "--manual-studio-completion", "done"]
            .into_iter().map(String::from).collect::<Vec<_>>();
        let options = super::LaunchCaptureOptions::parse(&args).unwrap();
        assert!(options.include_cursor);
        assert_eq!(options.shortcut.as_deref(), Some("Control+Shift+E"));
        assert!(super::launch_session(&args).unwrap().input.is_empty());
        assert!(!super::LaunchCaptureOptions::parse(&[]).unwrap().include_cursor);
        assert!(super::LaunchCaptureOptions::parse(&["--capture-shortcut".into()]).is_err());
        assert!(super::LaunchCaptureOptions::parse(&["--capture-shortcut".into(), "bad-key".into()]).is_err());
    }

    #[test]
    fn launch_capture_is_consumed_once_and_normal_launch_does_not_capture() {
        let request = StartupCapture(AtomicBool::new(true));
        assert!(request.take());
        assert!(!request.take());
        assert!(!StartupCapture(AtomicBool::new(false)).take());
    }

    #[test]
    fn capture_handoff_needs_no_input_but_editing_keeps_its_selected_image() {
        let capture: Vec<String> = ["--capture", "--manual-studio-output", "out.png", "--manual-studio-completion", "done"].into_iter().map(String::from).collect();
        let session = super::launch_session(&capture).unwrap();
        assert!(session.input.is_empty());
        assert_eq!(session.output, "out.png");
        assert_eq!(session.completion, "done");
        let mut edit = capture[1..].to_vec();
        assert!(super::launch_session(&edit).is_none());
        edit.extend(["--manual-studio-input".into(), "selected.png".into()]);
        assert_eq!(super::launch_session(&edit).unwrap().input, "selected.png");
        assert!(super::launch_session(&["--capture".into()]).is_none());
    }

    #[test]
    fn capture_cleanup_keeps_editor_alive_until_user_finishes_editing() {
        let state = super::AppState::default();
        assert!(!state.exit_on_capture_cancel().unwrap());
        *state.manual_studio.lock().unwrap() = Some(super::ManualStudioSession {
            input: String::new(), output: "capture.png".into(), completion: "done".into(),
        });
        assert!(state.exit_on_capture_cancel().unwrap(), "cancel before capture returns to Munin");
        state.capture_ready_for_editing.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(!state.exit_on_capture_cancel().unwrap(), "guide cleanup after capture must preserve the editor");
        state.capture_ready_for_editing.store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(state.exit_on_capture_cancel().unwrap(), "a new capture can still be cancelled");
    }
}

#[tauri::command]
fn cmd_manual_studio_session(app: AppHandle) -> Result<Option<ManualStudioSession>, String> {
    Ok(app.state::<AppState>().manual_studio.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
fn cmd_manual_studio_complete(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let session = state.manual_studio.lock().map_err(|e| e.to_string())?.clone().ok_or(crate::i18n::text("Manual Studio連携モードではありません", "Not in Manual Studio integration mode"))?;
    std::fs::write(session.completion, "saved").map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

const DEFAULT_CAPTURE_SHORTCUT: &str = "Control+Shift+E";

#[cfg(test)]
#[test]
fn holding_capture_key_does_not_take_multiple_screenshots() {
    let state = AppState::default();
    assert!(state.accept_capture_shortcut(true));
    assert!(!state.accept_capture_shortcut(true));
    assert!(!state.accept_capture_shortcut(true));
    assert!(!state.accept_capture_shortcut(false));
    assert!(state.accept_capture_shortcut(true));
}

fn valid_capture_shortcut(value: &str) -> bool {
    let special_key = value.split('+').next_back().is_some_and(|key| key == "PrintScreen"
        || key.strip_prefix('F').and_then(|number| number.parse::<u8>().ok()).is_some_and(|number| (1..=24).contains(&number)));
    (value.contains("Control+") || value.contains("Alt+") || value.contains("Super+") || value.contains("Meta+") || special_key)
        && value.parse::<Shortcut>().is_ok()
}

pub fn saved_capture_shortcut() -> String { read_shortcut() }

#[cfg(test)]
#[test]
fn capture_shortcuts_accept_custom_keys_without_stealing_plain_typing() {
    for key in ["Control+Shift+E", "Control+Alt+P", "PrintScreen", "Shift+F9"] {
        assert!(valid_capture_shortcut(key), "{key}");
    }
    for key in ["", "E", "Shift+E", "Control+", "not-a-key"] {
        assert!(!valid_capture_shortcut(key), "{key}");
    }
}

pub fn save_capture_shortcut(shortcut: &str) -> Result<(), String> {
    if !valid_capture_shortcut(shortcut) { return Err("Use a modifier key or a function key for capture".into()); }
    let path = shortcut_config_path();
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    std::fs::write(path, shortcut).map_err(|e| e.to_string())
}

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
        .map_err(|error| crate::localized!("UI探索階層の設定を保存できません: {error}", "Cannot save UI exploration depth: {error}"))?;
    ui_elements::set_exploration_depth(depth)?;
    Ok(depth)
}

fn read_shortcut() -> String {
    std::fs::read_to_string(shortcut_config_path())
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| valid_capture_shortcut(value))
        .unwrap_or_else(|| DEFAULT_CAPTURE_SHORTCUT.to_string())
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
    if !valid_capture_shortcut(&shortcut) {
        return Err("Unsupported capture shortcut".to_string());
    }
    let state = app.state::<AppState>();
    let previous = state.shortcut.lock().map_err(|e| e.to_string())?.clone();
    if previous != shortcut {
        state.capture_shortcut_down.store(false, std::sync::atomic::Ordering::SeqCst);
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
            return Err(crate::localized!("ショートカット設定を保存できません: {error}", "Cannot save shortcut settings: {error}"));
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
            let state = handle.state::<AppState>();
            if state.accept_capture_shortcut(event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed) {
                trigger_capture(&handle);
            }
        })
        .map_err(|e| crate::localized!("ショートカット {key} を登録できません: {e}", "Cannot register shortcut {key}: {e}"));
    *state.shortcut_warning.lock().map_err(|e| e.to_string())? = result.as_ref().err().cloned();
    result
}

pub fn run() {
    let _ = ui_elements::set_exploration_depth(read_exploration_depth());
    let args: Vec<String> = std::env::args().skip(1).collect();
    let launch_capture_options = match LaunchCaptureOptions::parse(&args) {
        Ok(options) => options,
        Err(error) => { eprintln!("{error}"); std::process::exit(2); }
    };
    if args.first().is_some_and(|arg| arg == "--manual-studio-record-input") {
        let result = args.get(1).ok_or_else(|| "Recorder output path is required".to_string())
            .and_then(|path| markits::recording::run_helper(std::path::Path::new(path)));
        if let Err(error) = result { eprintln!("{error}"); std::process::exit(2); }
        return;
    }
    let capture_requested = args.iter().any(|arg| arg == "--capture");
    let manual_studio = launch_session(&args);
    let startup_capture = args.iter().any(|arg| arg == "--capture") && manual_studio.is_none();
    let background = capture_requested || (args.iter().any(|arg| arg == "--background") && manual_studio.is_none());
    tauri::Builder::default()
        .manage(StartupCapture(std::sync::atomic::AtomicBool::new(startup_capture)))
        .manage(recording::RecordingState::default())
        .manage(playback::PlaybackState::default())
        .manage(AppState {
            shortcut: Mutex::new(launch_capture_options.shortcut.clone().unwrap_or_else(read_shortcut)),
            launch_capture_options,
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
            playback::cmd_playback_wait_mode,
            playback::cmd_playback_cursor_mode,
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
            cursor_capture::cmd_retry_cursor_capture_shortcut,
            cursor_capture::cmd_recover_cursor_capture,
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
            cmd_take_startup_capture,
            cmd_manual_studio_complete,
            i18n::cmd_set_locale
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
                        let _ = std::fs::write(marker, crate::i18n::text("MarkItsの編集を中止しました。元画像は保持しています。再度編集できます。", "Editing canceled. The original image is preserved and can be edited again."));
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
            let shortcut_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if shortcut_app.state::<AppState>().launch_capture_options.shortcut.is_some() { return; }
                let mut last = shortcut_app.state::<AppState>().shortcut.lock().map(|key| key.clone()).unwrap_or_default();
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let next = read_shortcut();
                    if next != last {
                        last = next.clone();
                        let _ = cmd_set_capture_shortcut(shortcut_app.clone(), next);
                    }
                }
            });
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
