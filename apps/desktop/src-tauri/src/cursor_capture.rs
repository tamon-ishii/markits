use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[tauri::command]
pub fn cmd_cursor_capture_supported() -> bool {
    cfg!(target_os = "windows")
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CursorCaptureTarget {
    UnderCursor,
    Window { window_id: String },
    Fullscreen,
    Region { x: u32, y: u32, width: u32, height: u32 },
}

fn resolve_capture_target(target: CursorCaptureTarget) -> Result<CursorCaptureTarget, String> {
    if target != CursorCaptureTarget::UnderCursor { return Ok(target); }
    #[cfg(target_os = "windows")]
    { Ok(CursorCaptureTarget::Window { window_id: windows::window_under_cursor()? }) }
    #[cfg(not(target_os = "windows"))]
    { Err("Cursor capture is only supported on Windows".into()) }
}

impl CursorCaptureTarget {
    fn window_id(&self) -> Option<&str> {
        match self {
            Self::Window { window_id } => Some(window_id),
            _ => None,
        }
    }

    pub fn bounds(&self) -> Result<markits::window_capture::WindowInfo, String> {
        if self == &Self::UnderCursor { return resolve_capture_target(self.clone())?.bounds(); }
        if let Some(id) = self.window_id() {
            return markits::window_capture::list_windows()?
                .into_iter()
                .find(|window| window.id == id)
                .ok_or_else(|| crate::i18n::text("撮影対象のウィンドウが閉じられました", "The capture target window was closed").into());
        }
        let screens = screenshots::Screen::all().map_err(|e| e.to_string())?;
        let screen = screens
            .first()
            .ok_or(crate::i18n::text("撮影する画面が見つかりません", "No screen available to capture"))?
            .display_info;
        if let Self::Region { x, y, width, height } = self {
            if *width == 0 || *height == 0 || x.checked_add(*width).is_none_or(|right| right > screen.width) || y.checked_add(*height).is_none_or(|bottom| bottom > screen.height) {
                return Err("Invalid capture region".into());
            }
            return Ok(markits::window_capture::WindowInfo {
                id: "region".into(), title: crate::i18n::text("選択範囲", "Selected region").into(),
                x: screen.x + *x as i32, y: screen.y + *y as i32, width: *width, height: *height,
            });
        }
        Ok(markits::window_capture::WindowInfo {
            id: "screen".into(),
            title: crate::i18n::text("全画面", "Full screen").into(),
            x: screen.x,
            y: screen.y,
            width: screen.width,
            height: screen.height,
        })
    }
}

#[derive(Clone, serde::Serialize)]
pub struct CursorGuideStatus {
    shortcut: String,
    window_title: String,
    error: Option<String>,
    warning: Option<String>,
    cancel_shortcut_available: bool,
    capture_shortcut_available: bool,
}

#[tauri::command]
pub fn cmd_cursor_guide_status(app: tauri::AppHandle) -> Result<CursorGuideStatus, String> {
    app.state::<crate::AppState>()
        .cursor_guide
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| crate::i18n::text("撮影の準備が完了していません", "Capture is not ready").into())
}

#[tauri::command]
pub fn cmd_retry_cursor_capture_shortcut(app: tauri::AppHandle) -> Result<CursorGuideStatus, String> {
    let warning=register_waiting_shortcut(&app)?;
    let state=app.state::<crate::AppState>();
    let key=state.shortcut.lock().map_err(|e|e.to_string())?.clone();
    let available=key.parse::<Shortcut>().is_ok_and(|key|app.global_shortcut().is_registered(key));
    let mut guide=state.cursor_guide.lock().map_err(|e|e.to_string())?;
    let status=guide.as_mut().ok_or("Cursor capture is not active")?;
    status.capture_shortcut_available=available;
    status.warning=warning;
    Ok(status.clone())
}

pub fn cancel(app: &tauri::AppHandle) {
    if let Ok(mut target) = app.state::<crate::AppState>().cursor_target.lock() {
        target.take();
    }
    if let Ok(mut guide) = app.state::<crate::AppState>().cursor_guide.lock() {
        guide.take();
    }
    if let Some(guide) = app.get_webview_window("cursor-guide") {
        let _ = guide.hide();
    }
    if let Ok(key) = "Escape".parse::<Shortcut>() {
        let _ = app.global_shortcut().unregister(key);
    }
}

#[tauri::command]
pub fn cmd_recover_cursor_capture(app: tauri::AppHandle, error: String) -> Result<(), String> {
    cancel(&app);
    if let Some(overlay) = app.get_webview_window("overlay") { let _ = overlay.hide(); }
    if let Some(main) = app.get_webview_window("main") {
        crate::commands::force_raise_window(&main);
        main.emit("capture-error", error).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cmd_arm_cursor_capture(
    app: tauri::AppHandle,
    target: CursorCaptureTarget,
) -> Result<(), String> {
    if app
        .state::<crate::AppState>()
        .cursor_capture_busy
        .swap(true, Ordering::SeqCst)
    {
        return Ok(());
    }
    let result = arm_cursor_capture(&app, target).await;
    app.state::<crate::AppState>()
        .cursor_capture_busy
        .store(false, Ordering::SeqCst);
    result
}

async fn arm_cursor_capture(
    app: &tauri::AppHandle,
    target: CursorCaptureTarget,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err(crate::i18n::text("Windows専用の撮影モードです", "This capture mode is available only on Windows").into());
    }
    if let Some(overlay) = app.get_webview_window("overlay") {
        overlay
            .set_cursor_icon(tauri::CursorIcon::Default)
            .map_err(|e| e.to_string())?;
        overlay
            .set_ignore_cursor_events(true)
            .map_err(|e| e.to_string())?;
        overlay.hide().map_err(|e| e.to_string())?;
        #[cfg(target_os = "windows")]
        {
            let hwnd = overlay.hwnd().map_err(|e| e.to_string())?.0 as usize;
            let (complete, wait) = std::sync::mpsc::channel();
            overlay
                .run_on_main_thread(move || {
                    windows::release_overlay_capture(hwnd);
                    let _ = complete.send(());
                })
                .map_err(|e| e.to_string())?;
            tauri::async_runtime::spawn_blocking(move || {
                wait.recv_timeout(std::time::Duration::from_secs(2))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| crate::localized!("選択画面のマウス入力を解除できません: {e}", "Cannot release mouse input from the selection overlay: {e}"))?;
        }
    }
    let mut shortcut_warning = register_waiting_shortcut(app)?;
    let escape = "Escape".parse::<Shortcut>().map_err(|e| e.to_string())?;
    let cancel_shortcut_available = app.global_shortcut()
        .on_shortcut(escape, |app, _, event| {
            if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let _ = crate::commands::cmd_cancel_capture(app.clone());
            }
        }).is_ok();
    if !cancel_shortcut_available {
        let note=crate::i18n::text("Escは他のアプリで使用中です。取り消す場合はキャンセルボタンを押してください。", "Esc is in use by another application. Use the Cancel button to cancel.");
        shortcut_warning=Some(shortcut_warning.map(|warning|format!("{warning}\n{note}")).unwrap_or_else(||note.into()));
    }
    let activate_id = target.window_id().map(str::to_owned);
    if let Some(id) = activate_id.clone() {
        let _ = tauri::async_runtime::spawn_blocking(move || markits::window_capture::activate_window(&id)).await;
    }
    let bounds = target.bounds().ok();
    let shortcut = app.state::<crate::AppState>().shortcut.lock().map_err(|e| e.to_string())?.clone();
    let capture_shortcut_available = shortcut.parse::<Shortcut>().is_ok_and(|key|app.global_shortcut().is_registered(key));
    let status = CursorGuideStatus {
        shortcut,
        window_title: if target == CursorCaptureTarget::UnderCursor {
            crate::i18n::text("キーを押した時点のカーソル下のウインドウ", "Window under the cursor when the key is pressed").into()
        } else { bounds.as_ref().map(|window| window.title.clone()).unwrap_or_default() },
        error: None, warning: shortcut_warning, cancel_shortcut_available,
        capture_shortcut_available,
    };
    *app.state::<crate::AppState>().cursor_guide.lock().map_err(|e| e.to_string())? = Some(status.clone());
    *app.state::<crate::AppState>().cursor_target.lock().map_err(|e| e.to_string())? = Some(target);
    if let Some(main) = app.get_webview_window("main") { main.hide().map_err(|e| e.to_string())?; }
    let guide = if let Some(guide) = app.get_webview_window("cursor-guide") { guide } else {
        tauri::WebviewWindowBuilder::new(app,"cursor-guide",tauri::WebviewUrl::App("index.html".into()))
            .title(crate::i18n::text("MarkIts — 撮影キー待機", "MarkIts — Waiting for capture key"))
            .inner_size(480.0,330.0).resizable(false).always_on_top(true)
            .focused(false).focusable(false).skip_taskbar(true).visible(false)
            .build().map_err(|e| e.to_string())?
    };
    let monitor = guide.available_monitors().ok().and_then(|monitors| bounds.as_ref().and_then(|bounds| {
        let x = bounds.x as i64 + bounds.width as i64 / 2;
        let y = bounds.y as i64 + bounds.height as i64 / 2;
        monitors.into_iter().find(|monitor| {
            let p=monitor.position();let size=monitor.size();
            x>=p.x as i64&&x<p.x as i64+size.width as i64&&y>=p.y as i64&&y<p.y as i64+size.height as i64
        })
    })).or_else(||guide.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        if let Ok(size) = guide.outer_size() {
            let p=monitor.position();let screen=monitor.size();
            guide.set_position(tauri::PhysicalPosition::new(p.x+(screen.width as i32-size.width as i32-24).max(0),p.y+(screen.height as i32-size.height as i32-72).max(0))).map_err(|e|e.to_string())?;
        }
    }
    guide.show().map_err(|e|e.to_string())?;
    let _ = guide.emit("cursor-guide-update", &status);
    if let Some(id) = activate_id {
        let _=tauri::async_runtime::spawn_blocking(move||markits::window_capture::activate_window(&id)).await;
    }
    #[cfg(target_os = "windows")]
    {
        windows::refresh_mouse_move();
        windows::refresh_cursor();
    }
    Ok(())
}

fn register_waiting_shortcut(app: &tauri::AppHandle) -> Result<Option<String>, String> {
    let requested = app.state::<crate::AppState>().shortcut.lock().map_err(|e| e.to_string())?.clone();
    match crate::register_capture_shortcuts(app) {
        Ok(()) => Ok(None),
        Err(error) => Ok(Some(crate::localized!(
            "設定された {requested} を登録できません。他のMarkItsなどが使用していないか確認してください。キーは変更していません。\n{error}",
            "Cannot register the configured {requested}. Check whether another MarkIts instance is using it. The key has not been changed.\n{error}"
        ))),
    }
}
/// Returns true while a pending pointer capture is handled, so repeated key presses
/// cannot reopen the selection overlay during capture.
pub async fn capture_pending(app: &tauri::AppHandle) -> Result<bool, String> {
    let state = app.state::<crate::AppState>();
    if state.cursor_capture_busy.swap(true, Ordering::SeqCst) {
        return Ok(true);
    }
    let result = async {
        let target = state
            .cursor_target
            .lock()
            .map_err(|e| e.to_string())?
            .take();
        let Some(id) = target else {
            return Ok(false);
        };
        let generation = state.capture_generation.load(Ordering::SeqCst);
        let capture_id = id.clone();
        let capture = async {
            if let Some(guide) = app.get_webview_window("cursor-guide") {
                guide.hide().map_err(|e| e.to_string())?;
            }
            let captured = tauri::async_runtime::spawn_blocking(move || {
                std::thread::sleep(std::time::Duration::from_millis(80));
                single_capture(&resolve_capture_target(capture_id)?)
            })
            .await
            .map_err(|e| e.to_string())??;
            if state.capture_generation.load(Ordering::SeqCst) != generation {
                return Ok(());
            }
            crate::commands::cmd_finish_capture(app.clone(), captured, None, Vec::new()).await
        }
        .await;
        if state.capture_generation.load(Ordering::SeqCst) != generation {
            return Ok(true);
        }
        match capture {
            Ok(()) => {
                cancel(app);
                // Hiding the guide can change foreground focus on Windows.
                // Raise the editor after the cursor-capture controls are gone.
                if let Some(main) = app.get_webview_window("main") {
                    crate::commands::force_raise_window(&main);
                }
            },
            Err(error) => {
                *state.cursor_target.lock().map_err(|e| e.to_string())? = Some(id);
                let status = {
                    let mut guide=state.cursor_guide.lock().map_err(|e|e.to_string())?;
                    if let Some(status)=guide.as_mut(){status.error=Some(error.clone());}
                    guide.clone()
                };
                if let Some(guide)=app.get_webview_window("cursor-guide") {
                    if let Some(status)=status {let _=guide.emit("cursor-guide-update",status);}
                    let _=guide.show();
                } else if let Some(main) = app.get_webview_window("main") {
                    crate::commands::force_raise_window(&main);
                    let _ = main.emit("capture-error", &error);
                }
            }
        }
        Ok(true)
    }
    .await;
    state.cursor_capture_busy.store(false, Ordering::SeqCst);
    result
}

pub fn snapshot_window(id: &str) -> Result<crate::capture::CapturedImage, String> {
    capture_window(id, true)
}

/// Small cursor-free sample for detecting redraw/transition completion.
pub(crate) fn sample_window_frame(window: &markits::window_capture::WindowInfo) -> Result<image::GrayImage, String> {
    #[cfg(target_os = "windows")]
    let pixels = image::DynamicImage::ImageRgba8(windows::screen_pixels(window)?);
    #[cfg(not(target_os = "windows"))]
    let pixels = image::DynamicImage::ImageRgb8(markits::window_capture::read_window_pixels(&window.id, None)?);
    Ok(pixels.resize_exact(96, 64, image::imageops::FilterType::Triangle).into_luma8())
}

pub fn snapshot_target(
    target: &CursorCaptureTarget,
) -> Result<crate::capture::CapturedImage, String> {
    snapshot_target_with_cursor(target, true)
}

pub fn snapshot_target_with_cursor(target: &CursorCaptureTarget, include_cursor: bool) -> Result<crate::capture::CapturedImage, String> {
    if target == &CursorCaptureTarget::UnderCursor { return snapshot_target_with_cursor(&resolve_capture_target(target.clone())?, include_cursor); }
    if let Some(id) = target.window_id() {
        return capture_window(id, include_cursor);
    }
    #[cfg(target_os = "windows")]
    {
        let bounds = target.bounds()?;
        let mut image = windows::screen_pixels(&bounds)?;
        if include_cursor { windows::draw_current_cursor(&mut image, &bounds)?; }
        let mut captured =
            crate::capture::rgba_to_captured_image(&image).map_err(|e| e.to_string())?;
        captured.ui_elements = crate::ui_elements::filter_elements_for_crop(
            &crate::ui_elements::capture_desktop_windows(0, 0),
            bounds.x as f64,
            bounds.y as f64,
            bounds.width as f64,
            bounds.height as f64,
        );
        captured.raw_png =
            crate::metadata::embed_ui_elements(&captured.raw_png, &captured.ui_elements)
                .map_err(|e| e.to_string())?;
        use base64::Engine;
        captured.data_url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&captured.raw_png)
        );
        Ok(captured)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err(crate::i18n::text("カーソル付き撮影はWindows専用です", "Cursor capture is available only on Windows").into())
    }
}

fn single_capture(target: &CursorCaptureTarget) -> Result<String, String> {
    if let Some(id) = target.window_id() {
        return crate::recording::single_pointer_capture(id);
    }
    let bounds = target.bounds()?;
    let mut pointer = pointer_in_bounds(&bounds)?;
    let captured = snapshot_target(target)?;
    #[cfg(target_os = "windows")]
    if let Some(pointer) = &mut pointer {
        pointer.target =
            markits::recording::named_target_at_point(pointer.x as i32, pointer.y as i32)
                .map(|(target, _)| target);
    }
    let saved = crate::playback::SavedRecording {
        capture_area: Some(target.clone()),
        window_id: String::new(),
        pid: 0,
        recording: markits::recording::OperationRecording {
            version: 1,
            window: bounds.title,
            bounds: [
                bounds.x as i64,
                bounds.y as i64,
                bounds.width as i64,
                bounds.height as i64,
            ],
            events: vec![markits::recording::RecordedEvent::Screenshot {
                elapsed_ms: 0,
                include_cursor: true,
                pointer,
            }],
        },
    };
    let png = crate::metadata::embed_text_chunk(
        &captured.raw_png,
        crate::playback::OPERATIONS_KEY,
        &serde_json::to_string(&saved).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    use base64::Engine;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

pub fn current_pointer(id: &str) -> Result<Option<markits::recording::RecordedPointer>, String> {
    let window = markits::window_capture::list_windows()?
        .into_iter()
        .find(|w| w.id == id)
        .ok_or(crate::i18n::text("撮影対象のウィンドウが見つかりません", "Capture target window not found"))?;
    pointer_in_bounds(&window)
}

pub(crate) fn pointer_in_bounds(
    window: &markits::window_capture::WindowInfo,
) -> Result<Option<markits::recording::RecordedPointer>, String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
        let mut position = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut position) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if position.x < window.x
            || position.y < window.y
            || position.x as f64 >= window.x as f64 + window.width as f64
            || position.y as f64 >= window.y as f64 + window.height as f64
        {
            return Ok(None);
        }
        Ok(Some(markits::recording::RecordedPointer {
            x: position.x as f64,
            y: position.y as f64,
            target: None,
            target_offset: None,
        }))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        Ok(None)
    }
}

#[cfg(target_os = "windows")]
pub fn snapshot_without_cursor(id: &str) -> Result<Vec<u8>, String> {
    capture_window(id, false).map(|captured| captured.raw_png)
}

fn capture_window(id: &str, include_cursor: bool) -> Result<crate::capture::CapturedImage, String> {
    #[cfg(target_os = "windows")]
    {
        let window = markits::window_capture::list_windows()?
            .into_iter()
            .find(|w| w.id == id)
            .ok_or(crate::i18n::text("撮影対象のウィンドウが見つかりません", "Capture target window not found"))?;
        let mut image = crate::playback_trace::measure("snapshot.read_pixels", || windows::screen_pixels(&window))?;
        if include_cursor {
            windows::draw_current_cursor(&mut image, &window)?;
        }
        let mut captured = crate::playback_trace::measure("snapshot.encode_png", ||
            crate::capture::rgba_to_captured_image(&image)).map_err(|e| e.to_string())?;
        let target = markits::semantic_target::window_context(id)?;
        captured.raw_png = crate::metadata::embed_text_chunk(
            &captured.raw_png,
            crate::metadata::MARKITS_CAPTURE_TARGET_KEYWORD,
            &serde_json::to_string(&target).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        use base64::Engine;
        captured.data_url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&captured.raw_png)
        );
        Ok(captured)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (id, include_cursor);
        Err(crate::i18n::text("マウスポインタ付き撮影はWindows専用です", "Pointer capture is available only on Windows").into())
    }
}

#[cfg(test)]
mod capture_target_tests {
    use super::*;

    #[test]
    fn fullscreen_cursor_recording_roundtrips_without_a_window_dependency() {
        let saved = crate::playback::SavedRecording {
            capture_area: Some(CursorCaptureTarget::Fullscreen),
            window_id: String::new(),
            pid: 0,
            recording: markits::recording::OperationRecording {
                version: 1,
                window: "Screen".into(),
                bounds: [0, 0, 1920, 1080],
                events: vec![markits::recording::RecordedEvent::Screenshot {
                    elapsed_ms: 0,
                    include_cursor: true,
                    pointer: Some(markits::recording::RecordedPointer {
                        x: 125.0,
                        y: 250.0,
                        target: None,
                        target_offset: None,
                    }),
                }],
            },
        };
        let restored: crate::playback::SavedRecording =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(restored.capture_area, Some(CursorCaptureTarget::Fullscreen));
        assert!(restored.window_id.is_empty());
        assert!(
            matches!(&restored.recording.events[0], markits::recording::RecordedEvent::Screenshot { include_cursor: true, pointer: Some(pointer), .. }
            if pointer.x == 125.0 && pointer.y == 250.0)
        );
    }
}

#[cfg(target_os = "windows")]
use markits::cursor_capture::windows;
