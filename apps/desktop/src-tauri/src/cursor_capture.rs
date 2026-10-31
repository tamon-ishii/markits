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
    Window { window_id: String },
    Fullscreen,
}

impl CursorCaptureTarget {
    fn window_id(&self) -> Option<&str> {
        match self {
            Self::Window { window_id } => Some(window_id),
            _ => None,
        }
    }

    pub fn bounds(&self) -> Result<markits::window_capture::WindowInfo, String> {
        if let Some(id) = self.window_id() {
            return markits::window_capture::list_windows()?
                .into_iter()
                .find(|window| window.id == id)
                .ok_or_else(|| "撮影対象のウィンドウが閉じられました".into());
        }
        let screens = screenshots::Screen::all().map_err(|e| e.to_string())?;
        let screen = screens
            .first()
            .ok_or("撮影する画面が見つかりません")?
            .display_info;
        Ok(markits::window_capture::WindowInfo {
            id: "screen".into(),
            title: "全画面".into(),
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
}

#[tauri::command]
pub fn cmd_cursor_guide_status(app: tauri::AppHandle) -> Result<CursorGuideStatus, String> {
    app.state::<crate::AppState>()
        .cursor_guide
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "撮影の準備が完了していません".into())
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
        return Err("Windows専用の撮影モードです".into());
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
            .map_err(|e| format!("選択画面のマウス入力を解除できません: {e}"))?;
        }
    }
    crate::register_capture_shortcuts(&app)?;
    let selected = target.clone();
    let window = tauri::async_runtime::spawn_blocking(move || {
        let window = selected.bounds()?;
        // Windows may refuse foreground activation. A visible window can still
        // be hovered and captured; refusal must not cancel keyboard capture.
        if let Some(id) = selected.window_id() {
            let _ = markits::window_capture::activate_window(id);
        }
        Ok::<_, String>(window)
    })
    .await
    .map_err(|e| e.to_string())??;
    let escape = "Escape".parse::<Shortcut>().map_err(|e| e.to_string())?;
    app.global_shortcut()
        .on_shortcut(escape, |app, _, event| {
            if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let _ = crate::commands::cmd_cancel_capture(app.clone());
            }
        })
        .map_err(|e| format!("撮影の取消キーを登録できません: {e}"))?;
    let activate_id = target.window_id().map(str::to_owned);
    *app.state::<crate::AppState>()
        .cursor_target
        .lock()
        .map_err(|e| e.to_string())? = Some(target);
    let status = CursorGuideStatus {
        shortcut: app
            .state::<crate::AppState>()
            .shortcut
            .lock()
            .map_err(|e| e.to_string())?
            .clone(),
        window_title: window.title,
        error: None,
    };
    *app.state::<crate::AppState>()
        .cursor_guide
        .lock()
        .map_err(|e| e.to_string())? = Some(status.clone());
    let guide = if let Some(guide) = app.get_webview_window("cursor-guide") {
        guide
    } else {
        let guide = tauri::WebviewWindowBuilder::new(
            app,
            "cursor-guide",
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title("MarkIts — マウスカーソル付き撮影")
        .inner_size(440.0, 260.0)
        .resizable(false)
        .always_on_top(true)
        .focused(false)
        .focusable(false)
        .skip_taskbar(true)
        .visible(false)
        .build();
        let guide = match guide {
            Ok(guide) => guide,
            Err(error) => {
                cancel(app);
                return Err(format!("撮影操作ウィンドウを表示できません: {error}"));
            }
        };
        guide
    };
    if let Ok(Some(monitor)) = guide.primary_monitor() {
        if let Ok(size) = guide.outer_size() {
            let position = monitor.position();
            let screen = monitor.size();
            let _ = guide.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
                position.x + (screen.width as i32 - size.width as i32 - 24).max(0),
                position.y + (screen.height as i32 - size.height as i32 - 72).max(0),
            )));
        }
    }
    let _ = guide.emit("cursor-guide-update", &status);
    if let Err(error) = guide.show() {
        cancel(app);
        return Err(error.to_string());
    }
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    // Creating/showing WebView2 can change focus despite focused(false). Return
    // it to the target only after the guide is visible, so live hover works.
    if let Some(id) = activate_id {
        let _ = tauri::async_runtime::spawn_blocking(move || {
            markits::window_capture::activate_window(&id)
        })
        .await;
    }
    #[cfg(target_os = "windows")]
    {
        windows::refresh_mouse_move();
        windows::refresh_cursor();
    }
    Ok(())
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
                single_capture(&capture_id)
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
            Ok(()) => cancel(app),
            Err(error) => {
                *state.cursor_target.lock().map_err(|e| e.to_string())? = Some(id);
                let status = {
                    let mut guide = state.cursor_guide.lock().map_err(|e| e.to_string())?;
                    if let Some(status) = guide.as_mut() {
                        status.error = Some(error);
                    }
                    guide.clone()
                };
                if let Some(main) = app.get_webview_window("main") {
                    let _ = main.hide();
                }
                if let Some(guide) = app.get_webview_window("cursor-guide") {
                    if let Some(status) = status {
                        let _ = guide.emit("cursor-guide-update", &status);
                    }
                    let _ = guide.show();
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

pub fn snapshot_target(
    target: &CursorCaptureTarget,
) -> Result<crate::capture::CapturedImage, String> {
    if let Some(id) = target.window_id() {
        return snapshot_window(id);
    }
    #[cfg(target_os = "windows")]
    {
        let bounds = target.bounds()?;
        let mut image = windows::screen_pixels(&bounds)?;
        windows::draw_current_cursor(&mut image, &bounds)?;
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
        Err("カーソル付き撮影はWindows専用です".into())
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
        .ok_or("撮影対象のウィンドウが見つかりません")?;
    pointer_in_bounds(&window)
}

fn pointer_in_bounds(
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
            .ok_or("撮影対象のウィンドウが見つかりません")?;
        let mut image = windows::screen_pixels(&window)?;
        if include_cursor {
            windows::draw_current_cursor(&mut image, &window)?;
        }
        let mut captured =
            crate::capture::rgba_to_captured_image(&image).map_err(|e| e.to_string())?;
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
        Err("マウスポインタ付き撮影はWindows専用です".into())
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
mod windows {
    use std::{
        mem::{size_of, zeroed},
        ptr::{copy_nonoverlapping, null_mut},
    };
    use windows_sys::Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::*};

    fn error() -> String {
        format!(
            "マウスポインタの撮影に失敗しました: {}",
            std::io::Error::last_os_error()
        )
    }

    pub fn release_overlay_capture(handle: usize) {
        use windows_sys::Win32::{Foundation::HWND, UI::Input::KeyboardAndMouse::ReleaseCapture};
        unsafe {
            let overlay = handle as HWND;
            let thread = GetWindowThreadProcessId(overlay, null_mut());
            let mut info: GUITHREADINFO = zeroed();
            info.cbSize = size_of::<GUITHREADINFO>() as u32;
            if GetGUIThreadInfo(thread, &mut info) != 0
                && !info.hwndCapture.is_null()
                && (GetAncestor(info.hwndCapture, GA_ROOT) == overlay
                    || IsChild(overlay, info.hwndCapture) != 0)
            {
                let _ = SendMessageTimeoutW(
                    info.hwndCapture,
                    WM_CANCELMODE,
                    0,
                    0,
                    SMTO_ABORTIFHUNG,
                    100,
                    null_mut(),
                );
            }
            ReleaseCapture();
        }
    }

    pub fn refresh_mouse_move() {
        unsafe {
            let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) != 0 {
                SetCursorPos(point.x, point.y);
            }
        }
    }

    pub fn refresh_cursor() {
        // Ask the window now under the pointer to choose its own cursor. Do not
        // force an arrow: text fields, links and resize borders need their shapes.
        unsafe {
            let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) == 0 {
                return;
            }
            let window = WindowFromPoint(point);
            if window.is_null() {
                return;
            }
            let location = ((point.y as u32 & 0xffff) << 16 | (point.x as u32 & 0xffff)) as isize;
            let mut hit = 0;
            if SendMessageTimeoutW(
                window,
                WM_NCHITTEST,
                0,
                location,
                SMTO_ABORTIFHUNG,
                100,
                &mut hit,
            ) == 0
            {
                return;
            }
            let reason = ((WM_MOUSEMOVE << 16) | (hit as u32 & 0xffff)) as isize;
            let _ = SendMessageTimeoutW(
                window,
                WM_SETCURSOR,
                window as usize,
                reason,
                SMTO_ABORTIFHUNG,
                100,
                null_mut(),
            );
        }
    }

    // Capture what is visible now, including hover tooltips which may be separate
    // native windows. PrintWindow/XCap's offscreen rendering can omit those.
    pub fn screen_pixels(
        window: &markits::window_capture::WindowInfo,
    ) -> Result<image::RgbaImage, String> {
        unsafe {
            let screen = GetDC(null_mut());
            if screen.is_null() {
                return Err(error());
            }
            let dc = CreateCompatibleDC(screen);
            if dc.is_null() {
                ReleaseDC(null_mut(), screen);
                return Err(error());
            }
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = window.width as i32;
            info.bmiHeader.biHeight = -(window.height as i32);
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() {
                DeleteDC(dc);
                ReleaseDC(null_mut(), screen);
                return Err(error());
            }
            let old = SelectObject(dc, bitmap);
            let result = if BitBlt(
                dc,
                0,
                0,
                window.width as i32,
                window.height as i32,
                screen,
                window.x,
                window.y,
                SRCCOPY | CAPTUREBLT,
            ) == 0
            {
                Err(error())
            } else {
                GdiFlush();
                let mut image = image::RgbaImage::new(window.width, window.height);
                let pixels = std::slice::from_raw_parts(bits as *const u8, image.as_raw().len());
                for (pixel, raw) in image.pixels_mut().zip(pixels.chunks_exact(4)) {
                    *pixel = image::Rgba([raw[2], raw[1], raw[0], 255]);
                }
                Ok(image)
            };
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            ReleaseDC(null_mut(), screen);
            result
        }
    }

    pub fn draw_current_cursor(
        image: &mut image::RgbaImage,
        window: &markits::window_capture::WindowInfo,
    ) -> Result<(), String> {
        unsafe {
            let mut cursor: CURSORINFO = zeroed();
            cursor.cbSize = size_of::<CURSORINFO>() as u32;
            if GetCursorInfo(&mut cursor) == 0 {
                return Err(error());
            }
            if cursor.flags & CURSOR_SHOWING == 0 {
                return Ok(());
            }
            let icon = CopyIcon(cursor.hCursor);
            if icon.is_null() {
                return Err(error());
            }
            let mut info: ICONINFO = zeroed();
            if GetIconInfo(icon, &mut info) == 0 {
                DestroyIcon(icon);
                return Err(error());
            }
            let sx = image.width() as f64 / window.width.max(1) as f64;
            let sy = image.height() as f64 / window.height.max(1) as f64;
            let x = ((cursor.ptScreenPos.x as f64 - window.x as f64 - info.xHotspot as f64) * sx)
                .round() as i32;
            let y = ((cursor.ptScreenPos.y as f64 - window.y as f64 - info.yHotspot as f64) * sy)
                .round() as i32;
            let mut bitmap: BITMAP = zeroed();
            let source = if !info.hbmColor.is_null() {
                info.hbmColor
            } else {
                info.hbmMask
            };
            let result = if GetObjectW(
                source,
                size_of::<BITMAP>() as i32,
                &mut bitmap as *mut _ as _,
            ) == 0
            {
                Err(error())
            } else {
                let height = if info.hbmColor.is_null() {
                    bitmap.bmHeight / 2
                } else {
                    bitmap.bmHeight
                };
                draw_icon(
                    image,
                    icon,
                    x,
                    y,
                    (bitmap.bmWidth as f64 * sx).round() as i32,
                    (height as f64 * sy).round() as i32,
                )
            };
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
            DestroyIcon(icon);
            result
        }
    }

    // Draw onto the actual background, preserving monochrome cursors' AND/XOR masks.
    fn draw_icon(
        image: &mut image::RgbaImage,
        icon: HICON,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return Err(error());
            }
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = image.width() as i32;
            info.bmiHeader.biHeight = -(image.height() as i32);
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() {
                DeleteDC(dc);
                return Err(error());
            }
            let old = SelectObject(dc, bitmap);
            let mut bgra = image.as_raw().clone();
            for pixel in bgra.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
            let drawn = DrawIconEx(dc, x, y, icon, width, height, 0, null_mut(), DI_NORMAL);
            GdiFlush();
            if drawn != 0 {
                copy_nonoverlapping(bits as *const u8, bgra.as_mut_ptr(), bgra.len());
                for (pixel, raw) in image.pixels_mut().zip(bgra.chunks_exact(4)) {
                    *pixel = image::Rgba([raw[2], raw[1], raw[0], 255]);
                }
            }
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            if drawn == 0 { Err(error()) } else { Ok(()) }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn cursor_shapes_and_clipping_are_rendered_on_the_background() {
            let background =
                image::RgbaImage::from_pixel(96, 96, image::Rgba([120, 180, 220, 255]));
            let arrow = unsafe { LoadCursorW(null_mut(), IDC_ARROW) };
            let beam = unsafe { LoadCursorW(null_mut(), IDC_IBEAM) };
            assert!(!arrow.is_null() && !beam.is_null());
            let mut a = background.clone();
            let mut b = background.clone();
            draw_icon(&mut a, arrow, 20, 20, 32, 32).unwrap();
            draw_icon(&mut b, beam, 20, 20, 32, 32).unwrap();
            assert_ne!(a, background);
            assert_ne!(b, background);
            assert_ne!(a, b);
            assert!(a.pixels().all(|p| p[3] == 255));
            let mut outside = background.clone();
            draw_icon(&mut outside, arrow, -100, -100, 32, 32).unwrap();
            assert_eq!(outside, background);
        }
    }
}
