use crate::{history, metadata};
use base64::Engine;
use markits::recording::{OperationRecording, RecordedEvent};
use serde::{Deserialize, Serialize};
#[cfg(any(test, not(target_os = "windows")))]
use std::io::Cursor;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tauri::{Emitter, Manager};

pub const OPERATIONS_KEY: &str = "markits:operations";
pub const GROUP_KEY: &str = "markits:operation_group";

pub fn copy_metadata(target: &[u8], source: &[u8]) -> Result<Vec<u8>, metadata::MetadataError> {
    let mut result = target.to_vec();
    if source.starts_with(b"\x89PNG\r\n\x1a\n") {
        for key in [OPERATIONS_KEY, GROUP_KEY, markits::recording_metadata::RECIPE_KEY, metadata::MARKITS_CAPTURE_TARGET_KEYWORD] {
            if let Some(value) = metadata::extract_text_chunk(source, key)? {
                result = metadata::embed_text_chunk(&result, key, &value)?;
            }
        }
    }
    Ok(result)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SavedRecording {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_area: Option<crate::cursor_capture::CursorCaptureTarget>,
    pub recording: OperationRecording,
    pub window_id: String,
    pub pid: u32,
}

pub struct PlaybackState(pub AtomicBool, pub AtomicBool, pub Mutex<PlaybackProgress>, pub AtomicBool, pub Mutex<u8>);

impl Default for PlaybackState {
    fn default() -> Self {
        Self(AtomicBool::new(false), AtomicBool::new(false),
            Mutex::new(PlaybackProgress { shorten_waits: true, ..Default::default() }),
            AtomicBool::new(false), Mutex::new(0))
    }
}

#[derive(Default, Clone, Serialize)]
pub struct PlaybackProgress {
    running: bool,
    step: usize,
    total: usize,
    message: String,
    completed: Vec<String>,
    operations: Vec<String>,
    waits_ms: Vec<u64>,
    captures: usize,
    error: Option<String>,
    target_image: Option<String>,
    target_description: Option<String>,
    paused: bool,
    wait_total_ms: u64,
    wait_remaining_ms: u64,
    shorten_waits: bool,
    include_cursor: bool,
    #[serde(skip)]
    cursor_override: Option<bool>,
}

#[tauri::command]
pub fn cmd_playback_cursor_mode(state: tauri::State<'_, PlaybackState>, enabled: bool) -> Result<(), String> {
    if enabled && !cfg!(target_os = "windows") {
        return Err(crate::i18n::text("ポインタ付き撮影はWindows専用です", "Pointer capture is available only on Windows").into());
    }
    let mut progress = state.2.lock().map_err(|e| e.to_string())?;
    progress.cursor_override = Some(enabled);
    progress.include_cursor = enabled;
    Ok(())
}

#[tauri::command]
pub fn cmd_playback_wait_mode(state: tauri::State<'_, PlaybackState>, shorten_waits: bool) -> Result<(), String> {
    state.2.lock().map_err(|e| e.to_string())?.shorten_waits = shorten_waits;
    Ok(())
}

fn operation_wait(configured_ms: u64, shorten_waits: bool, _is_screenshot: bool) -> u64 {
    if shorten_waits { 0 } else { configured_ms }
}

fn screenshot_settle_wait(shorten_waits: bool, moved_pointer: bool, action_settled: bool) -> u64 {
    if shorten_waits && !action_settled { 1000 } else if moved_pointer { 300 } else { 0 }
}

fn post_operation_wait(event: &RecordedEvent, shorten_waits: bool) -> u64 {
    if matches!(event, RecordedEvent::Click { .. }) ||
        (shorten_waits && !matches!(event, RecordedEvent::Screenshot { .. })) { 1000 } else { 0 }
}

#[tauri::command]
pub fn cmd_playback_capture_wait(state: tauri::State<'_, PlaybackState>, index: usize, milliseconds: u64) -> Result<(), String> {
    let mut progress = state.2.lock().map_err(|e| e.to_string())?;
    *progress.waits_ms.get_mut(index).ok_or(crate::i18n::text("操作が見つかりません", "Operation not found"))? = if index == 0 { 0 } else { milliseconds.min(3_600_000) };
    Ok(())
}

#[tauri::command]
pub fn cmd_playback_control(state: tauri::State<'_, PlaybackState>, action: String, shorten_waits: Option<bool>) -> Result<(), String> {
    let mut mode = state.4.lock().map_err(|e| e.to_string())?;
    let next_mode = match action.as_str() { "pause" => 1, "resume" => 0, "step" => 2, _ => return Err(crate::i18n::text("不明な操作です", "Unknown operation").into()) };
    // Commit the displayed delay mode before releasing the playback worker.
    if next_mode != 1 {
        if let Some(shorten_waits) = shorten_waits {
            state.2.lock().map_err(|e| e.to_string())?.shorten_waits = shorten_waits;
        }
    }
    *mode = next_mode;
    Ok(())
}

fn wait_for_step(app: &tauri::AppHandle) -> Result<(), String> {
    let _timing = crate::playback_trace::Span::new("control.wait_for_play");
    loop {
        let state = app.state::<PlaybackState>();
        if state.1.load(Ordering::SeqCst) { return Err(crate::i18n::text("再撮影を停止しました", "Recapture stopped").into()); }
        let mut mode = state.4.lock().map_err(|e| e.to_string())?;
        let paused = *mode == 1;
        progress(app, |value| value.paused = paused);
        if !paused { if *mode == 2 { *mode = 1; } return Ok(()); }
        drop(mode);
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_retry(app: &tauri::AppHandle, error: String) -> Result<(), String> {
    if app.state::<PlaybackState>().1.load(Ordering::SeqCst) { return Err(crate::i18n::text("再撮影を停止しました", "Recapture stopped").into()); }
    let popup = app.get_webview_window("playback");
    let was_hidden = popup.as_ref().and_then(|w| w.is_visible().ok()) == Some(false);
    if was_hidden { if let Some(w) = &popup { let _ = w.show(); } }
    *app.state::<PlaybackState>().4.lock().map_err(|e| e.to_string())? = 1;
    progress(app, |value| { value.error = Some(error); value.paused = true; });
    let result = wait_for_step(app);
    if was_hidden { if let Some(w) = &popup { let _ = w.hide(); thread::sleep(Duration::from_millis(80)); } }
    result?;
    progress(app, |value| value.error = None);
    Ok(())
}

fn activate_for_replay(app: &tauri::AppHandle, window_id: &str) -> Result<(), String> {
    let _timing = crate::playback_trace::Span::new("window.activate");
    loop {
        match markits::window_capture::activate_window(window_id) {
            Ok(_) => return Ok(()),
            Err(error) => wait_for_retry(app, crate::localized!("対象ウィンドウを操作できません: {error}", "Cannot control the target window: {error}"))?,
        }
    }
}

#[tauri::command]
pub fn cmd_playback_ready(state: tauri::State<'_, PlaybackState>) {
    state.3.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn cmd_playback_status(
    state: tauri::State<'_, PlaybackState>,
) -> Result<PlaybackProgress, String> {
    state
        .2
        .lock()
        .map(|value| value.clone())
        .map_err(|e| e.to_string())
}

fn progress(app: &tauri::AppHandle, update: impl FnOnce(&mut PlaybackProgress)) {
    if let Ok(mut value) = app.state::<PlaybackState>().2.lock() {
        update(&mut value);
    }
}

fn describe(event: &RecordedEvent) -> String {
    match event {
        RecordedEvent::Click { x, y, target, .. } => target
            .as_ref()
            .map(|target| crate::localized!("クリック【名称で照合】：{}（{}）", "Click [match by name]: {} ({})", target.name, target.role))
            .unwrap_or_else(|| crate::localized!("クリック【座標のみ】：({x:.0}, {y:.0})", "Click [coordinates only]: ({x:.0}, {y:.0})")),
        RecordedEvent::Text { value, .. } => {
            crate::localized!("文字入力：{}", "Text input: {}", value.chars().take(60).collect::<String>())
        }
        RecordedEvent::Key { value, .. } => crate::localized!("キー：{value}", "Key: {value}"),
        RecordedEvent::Scroll { dx, dy, .. } => crate::localized!("スクロール：横 {dx} / 縦 {dy}", "Scroll: horizontal {dx} / vertical {dy}"),
        RecordedEvent::Screenshot { include_cursor: true, pointer, .. } => crate::localized!("ポインタ付き撮影：{}", "Capture with pointer: {}",
            pointer.as_ref().map(|p| p.target.as_ref().map(|target| target.name.clone())
                .unwrap_or_else(|| format!("({:.0}, {:.0})", p.x, p.y))).unwrap_or_else(|| crate::i18n::text("現在のウィンドウ", "Current window").into())),
        RecordedEvent::Screenshot { .. } => crate::i18n::text("キャプチャ：現在のウィンドウを新しく撮影", "Capture: take a new screenshot of the current window").into(),
    }
}

// Hide the progress window while sending pointer input or taking a screenshot,
// so it neither intercepts the click nor appears in the captured pixels.
fn without_popup<T>(
    app: &tauri::AppHandle,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    without_popup_restore(app, true, action)
}

fn without_popup_restore<T>(app: &tauri::AppHandle, restore: bool, action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let popup = app.get_webview_window("playback");
    if let Some(popup) = &popup {
        if popup.is_visible().unwrap_or(true) {
            let _timing = crate::playback_trace::Span::new("popup.hide");
            popup.hide().map_err(|e| e.to_string())?;
            thread::sleep(Duration::from_millis(80));
        }
    }
    let result = action();
    if restore || result.is_err() {
        if let Some(popup) = popup { let _ = popup.show(); }
    }
    result
}

pub fn extract(bytes: &[u8]) -> Result<Option<SavedRecording>, String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(None);
    }
    // Exported PNGs keep the original image, including its recording metadata.
    let mut bytes = bytes.to_vec();
    for _ in 0..4 {
        if let Some(json) =
            metadata::extract_text_chunk(&bytes, OPERATIONS_KEY).map_err(|e| e.to_string())?
        {
            let mut saved: SavedRecording = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            // Repair frames produced before cursor overrides normalized live
            // screen coordinates back into the original recording space.
            if let Some(target) = metadata::extract_text_chunk(&bytes, metadata::MARKITS_CAPTURE_TARGET_KEYWORD)
                .ok().flatten().and_then(|json| serde_json::from_str::<crate::ui_elements::DetectedUiElement>(&json).ok()) {
                let recorded = saved.recording.bounds.map(|v| v as f64);
                let current = [target.x, target.y, target.width, target.height];
                for event in &mut saved.recording.events {
                    if let RecordedEvent::Screenshot { pointer: Some(pointer), .. } = event {
                        if !point_in_bounds(pointer.x, pointer.y, recorded) && point_in_bounds(pointer.x, pointer.y, current) {
                            *pointer = pointer_in_recording_space(pointer.clone(), saved.recording.bounds, current);
                        }
                    }
                }
            }
            return Ok(Some(saved));
        }
        let Some(source) = metadata::extract_text_chunk(&bytes, metadata::MARKITS_SOURCE_KEYWORD)
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        bytes = decode(&source)?;
    }
    Ok(None)
}

fn decode(url: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(url.split_once(',').ok_or("Invalid image data")?.1)
        .map_err(|e| e.to_string())
}

pub fn snapshot(window_id: &str) -> Result<Vec<u8>, String> {
    let _timing = crate::playback_trace::Span::new("snapshot.total");
    #[cfg(target_os = "windows")]
    { crate::commands::attach_window_snap_metadata(crate::cursor_capture::snapshot_without_cursor(window_id)?, window_id) }
    #[cfg(not(target_os = "windows"))]
    {
    let pixels = markits::window_capture::read_window_pixels(window_id, None)?;
    let mut png = Cursor::new(Vec::new());
    pixels
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    crate::commands::attach_window_snap_metadata(png.into_inner(), window_id)
    }
}

pub fn save_snapshots(
    saved: &SavedRecording,
    pictures: Vec<Vec<u8>>,
) -> Result<Vec<metadata::LoadedImageResult>, String> {
    let _timing = crate::playback_trace::Span::new("history.save_snapshots");
    let group = format!(
        "operations_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    );
    let json = serde_json::to_string(saved).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for bytes in pictures {
        let bytes =
            metadata::embed_text_chunk(&bytes, OPERATIONS_KEY, &json).map_err(|e| e.to_string())?;
        let bytes =
            metadata::embed_text_chunk(&bytes, GROUP_KEY, &group).map_err(|e| e.to_string())?;
        let item =
            history::save_capture_to_history(&bytes, None, None).map_err(|e| e.to_string())?;
        let mut loaded = metadata::load_image_with_metadata(&bytes).map_err(|e| e.to_string())?;
        loaded.history_id = Some(item.id);
        results.push(loaded);
    }
    Ok(results)
}

#[tauri::command]
pub fn cmd_has_operations(image_data_url: String) -> Result<bool, String> {
    Ok(extract(&decode(&image_data_url)?)?.is_some())
}

#[tauri::command]
pub fn cmd_playback_cancel(state: tauri::State<'_, PlaybackState>) {
    state.1.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn cmd_playback(
    app: tauri::AppHandle,
    image_data_url: String,
    max_wait_ms: Option<u64>,
    shorten_waits: Option<bool>,
) -> Result<Vec<metadata::LoadedImageResult>, String> {
    let saved = extract(&decode(&image_data_url)?)?.ok_or(crate::i18n::text("この画像には操作記録がありません", "This image has no recorded operations"))?;
    let _ = max_wait_ms;
    let max_wait_ms = u64::MAX;
    app.state::<crate::recording::RecordingState>()
        .claim_playback(&app.state::<PlaybackState>())?;
    app.state::<PlaybackState>().3.store(false, Ordering::SeqCst);
    *app.state::<PlaybackState>().4.lock().map_err(|e| e.to_string())? = 1;
    app.state::<PlaybackState>()
        .1
        .store(false, Ordering::SeqCst);
    progress(&app, |value| {
        let shorten_waits = shorten_waits.unwrap_or(value.shorten_waits);
        let cursor_override = value.cursor_override;
        let recorded_cursor = saved.recording.events.iter().any(|event|
            matches!(event, RecordedEvent::Screenshot { include_cursor: true, .. }));
        *value = PlaybackProgress {
            shorten_waits,
            include_cursor: cursor_override.unwrap_or(recorded_cursor),
            cursor_override,
            running: true,
            total: saved.recording.events.len(),
            operations: saved.recording.events.iter().map(describe).collect(),
            waits_ms: recorded_waits(&saved.recording.events),
            paused: true,
            message: crate::i18n::text("待機時間を確認し、再生を押してください", "Check the delays, then press Play").into(),
            ..Default::default()
        }
    });
    let popup = if let Some(existing) = app.get_webview_window("playback") {
        existing.show().map(|_| existing)
    } else { tauri::WebviewWindowBuilder::new(
        &app,
        "playback",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title(crate::i18n::text("MarkIts — 再生・再撮影", "MarkIts — Playback & Recapture"))
    .inner_size(480.0, 760.0)
    .always_on_top(true)
    .focused(false)
    .focusable(true)
    .build() };
    if let Err(error) = popup {
        app.state::<PlaybackState>()
            .0
            .store(false, Ordering::SeqCst);
        progress(&app, |value| {
            value.running = false;
            value.error = Some(error.to_string());
        });
        return Err(error.to_string());
    }
    let host = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || replay(&host, saved, max_wait_ms))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    app.state::<PlaybackState>()
        .0
        .store(false, Ordering::SeqCst);
    progress(&app, |value| {
        value.running = false;
        value.wait_remaining_ms = 0;
        match &result {
            Ok(images) => {
                value.captures = images.len();
                value.message = crate::localized!("完了：新しく {} 枚を撮影しました", "Completed: {} new images captured", images.len());
            }
            Err(error) => {
                value.message = crate::localized!("再生を停止しました（操作 {} / {}）", "Playback stopped (operation {} / {})", value.step, value.total);
                value.error = Some(error.clone());
            }
        }
    });
    if let Some(popup) = app.get_webview_window("playback") {
        let _ = popup.show();
        let _ = popup.set_focusable(true);
    }
    if let Some(main) = app.get_webview_window("main") {
        crate::commands::force_raise_window(&main);
    }
    result
}

fn replay(
    app: &tauri::AppHandle,
    mut saved: SavedRecording,
    max_wait_ms: u64,
) -> Result<Vec<metadata::LoadedImageResult>, String> {
    let _trace = crate::playback_trace::Run::start();
    let _timing = crate::playback_trace::Span::new("replay.total");
    let _depth = markits::ui_elements::capture_depth_scope(128);
    let ready_timing = crate::playback_trace::Span::new("frontend.ready");
    let ready_started = std::time::Instant::now();
    while !app.state::<PlaybackState>().3.load(Ordering::SeqCst) {
        if app.state::<PlaybackState>().1.load(Ordering::SeqCst) { return Err(crate::i18n::text("再生を停止しました", "Playback stopped").into()); }
        if ready_started.elapsed() > Duration::from_secs(10) { return Err(crate::i18n::text("操作リストの表示が完了しませんでした", "The operation list did not finish loading").into()); }
        thread::sleep(Duration::from_millis(20));
    }
    if saved.recording.version != 1 {
        return Err(crate::i18n::text("未対応の操作記録バージョンです", "Unsupported recording version").into());
    }
    drop(ready_timing);
    if let Some(area) = saved.capture_area.clone() {
        return replay_fullscreen(app, saved, area, max_wait_ms);
    }
    let windows = markits::window_capture::list_windows()?;
    let owners = markits::window_capture::window_process_ids()?;
    let original = windows.iter().find(|w| {
        w.id == saved.window_id
            && owners
                .iter()
                .any(|(id, pid)| id == &w.id && *pid == saved.pid)
    });
    let window = if let Some(window) = original {
        window
    } else {
        let matches: Vec<_> = windows
            .iter()
            .filter(|w| w.title == saved.recording.window)
            .collect();
        if matches.len() != 1 {
            return Err(
                crate::i18n::text("対象ウインドウを一意に特定できません。対象を開いてから再生してください", "Cannot identify a unique target window. Open the target and try again.").into(),
            );
        }
        matches[0]
    };
    // Validate all keyboard events before executing any input.
    for event in &saved.recording.events {
        if let RecordedEvent::Key { value, .. } = event {
            parse_chord(value)?;
        }
    }
    if let Some(main) = app.get_webview_window("main") {
        main.hide().map_err(|e| e.to_string())?;
    }
    let check = || -> Result<(), String> {
        loop {
            if app.state::<PlaybackState>().1.load(Ordering::SeqCst) {
                return Err(crate::i18n::text("再生を停止しました", "Playback stopped").into());
            }
            match markits::window_capture::is_window_active(&window.id) {
                Ok(true) => return Ok(()),
                Ok(false) => wait_for_retry(app, crate::i18n::text("対象ウィンドウからフォーカスが移動しました。対象を確認して再生してください", "The target window lost focus. Check the target, then press Play.").into())?,
                Err(error) => wait_for_retry(app, crate::localized!("対象ウィンドウを確認できません: {error}", "Cannot check the target window: {error}"))?,
            }
            activate_for_replay(app, &window.id)?;
        }
    };
    let mut pictures = Vec::new();
    let mut captured_cursor_modes = Vec::new();
    let action_settled = std::cell::Cell::new(false);
    let next_is_screenshot = std::cell::Cell::new(false);
    play_events(
        &saved.recording.events,
        max_wait_ms,
        |_, index| {
            next_is_screenshot.set(matches!(saved.recording.events.get(index + 1), Some(RecordedEvent::Screenshot { .. })));
            wait_for_step(app)?;
            activate_for_replay(app, &window.id)?;
            let delay = {
                let state = app.state::<PlaybackState>();
                let progress = state.2.lock().map_err(|e| e.to_string())?;
                operation_wait(progress.waits_ms[index], progress.shorten_waits,
                    matches!(saved.recording.events[index], RecordedEvent::Screenshot { .. }))
            };
            progress(app, |value| {
                value.step = index + 1;
                let event = &saved.recording.events[index];
                value.target_image = None;
                value.target_description = match event {
                    RecordedEvent::Click { x, y, target, .. } => Some(crate::localized!("対象ウィンドウ：{} ／ 記録時の位置：({x:.0}, {y:.0}) ／ {}", "Target window: {} / Recorded position: ({x:.0}, {y:.0}) / {}",
                        saved.recording.window,
                        target.as_ref().map(|t| crate::localized!("操作対象：{}", "Target: {}", t.name))
                            .unwrap_or_else(|| crate::i18n::text("記録座標を使用", "Use recorded coordinates").into())
                    )),
                    _ => None,
                };
                value.message = if delay > 0 {
                    crate::localized!("待機中（{:.1} 秒）：{}", "Waiting ({:.1} s): {}",
                        delay as f64 / 1000.0,
                        describe(&saved.recording.events[index])
                    )
                } else {
                    describe(&saved.recording.events[index])
                };
            });
            countdown(app, delay, &check)?;
            check()?;
            let _ = app.emit("playback-progress", index + 1);
            Ok(())
        },
        |event| {
            progress(app, |value| {
                value.message = crate::localized!("実行中：{}", "Running: {}", describe(event))
            });
            match event {
                RecordedEvent::Screenshot { include_cursor, pointer, .. } => {
                    let include_cursor = app.state::<PlaybackState>().2.lock().map_err(|e| e.to_string())?
                        .cursor_override.unwrap_or(*include_cursor);
                    let shorten_waits = app.state::<PlaybackState>().2.lock().map_err(|e| e.to_string())?.shorten_waits;
                    let point = if let Some(pointer) = pointer {
                        let current = markits::window_capture::list_windows()?.into_iter()
                            .find(|w| w.id == window.id).ok_or(crate::i18n::text("対象ウィンドウが閉じられました", "The target window was closed"))?;
                        loop {
                            let result = if pointer.target.is_some() { crate::playback_trace::measure("ui.observe_pointer_target", || markits::semantic_target::observe_window(&window.id)) } else { Ok(Vec::new()) }
                                .and_then(|elements| hover_point(pointer, &saved.recording, &current, &elements));
                            match result {
                                Ok(point) => break Some(point),
                                Err(error) => { wait_for_retry(app, error)?; activate_for_replay(app, &window.id)?; }
                            }
                        }
                    } else { None };
                    pictures.push(without_popup(app, || {
                        if let Some((x, y)) = point {
                            check()?;
                            send(rdev::EventType::MouseMove { x, y })?;
                        }
                        wait_before_screenshot(app, shorten_waits, point.is_some(), action_settled.replace(false), &check)?;
                        wait_for_visual_ready(app, window, None, &check)?;
                        let recorded_pointer = if include_cursor && pointer.is_none() {
                            crate::cursor_capture::current_pointer(&window.id)?.map(|pointer|
                                pointer_in_recording_space(pointer, saved.recording.bounds,
                                    [window.x as f64, window.y as f64, window.width as f64, window.height as f64]))
                        } else { pointer.clone() };
                        let png = if include_cursor { crate::commands::attach_window_snap_metadata(crate::cursor_capture::snapshot_window(&window.id)?.raw_png, &window.id)? }
                        else { snapshot(&window.id)? };
                        captured_cursor_modes.push((include_cursor, recorded_pointer));
                        Ok(png)
                    })?)
                }
                RecordedEvent::Click {
                    x,
                    y,
                    target,
                    ..
                } => {
                    let current = markits::window_capture::list_windows()?
                        .into_iter()
                        .find(|w| w.id == window.id)
                        .ok_or(crate::i18n::text("対象ウインドウが閉じられました", "The target window was closed"))?;
                    let point = if let Some(target) = target {
                        let element = loop {
                            let result = crate::playback_trace::measure("ui.observe_click_target", || markits::semantic_target::observe_window(&window.id))
                                .and_then(|elements| target.resolve(&elements).map(|element| element.map(|e| (e.x + e.width / 2.0, e.y + e.height / 2.0))))
                                .and_then(|element| element.ok_or_else(|| crate::localized!("操作対象が見つかりません: {}", "Target element not found: {}", target.name)));
                            match result {
                                Ok(element) => break element,
                                Err(error) => {
                                    wait_for_retry(app, error)?;
                                    activate_for_replay(app, &window.id)?;
                                }
                            }
                        };
                        element
                    } else {
                        relative_point(*x, *y, &saved.recording, &current)?
                    };
                    check()?;
                    without_popup_restore(app, !next_is_screenshot.get(), || {
                        send(rdev::EventType::MouseMove {
                            x: point.0,
                            y: point.1,
                        })?;
                        let before_navigation = if target.as_ref().is_some_and(|target| target.role == "link") {
                            Some(crate::cursor_capture::sample_window_frame(&current)?)
                        } else { None };
                        send(rdev::EventType::ButtonPress(rdev::Button::Left))?;
                        send(rdev::EventType::ButtonRelease(rdev::Button::Left))?;
                        // Start settling immediately after releasing the click,
                        // before showing the popup or executing another event.
                        let click_timing = crate::playback_trace::Span::with_expected("click.after_wait", Some(1000));
                        progress(app, |value| value.message = crate::i18n::text(
                            "クリック後の画面切り替えを待っています（1秒）",
                            "Waiting for the screen transition after clicking (1 second)").into());
                        countdown(app, post_operation_wait(event, false), &check)?;
                        drop(click_timing);
                        if let Some(before) = before_navigation.as_ref() {
                            loop {
                                match wait_for_visual_ready(app, &current, Some(before), &check) {
                                    Ok(()) => break,
                                    Err(error) => { wait_for_retry(app, error)?; activate_for_replay(app, &window.id)?; }
                                }
                            }
                        }
                        action_settled.set(true);
                        Ok(())
                    })?;
                }
                RecordedEvent::Text { value, .. } => paste(value)?,
                RecordedEvent::Key { value, .. } => chord(&parse_chord(value)?)?,
                RecordedEvent::Scroll { x, y, dx, dy, .. } => {
                    let current = markits::window_capture::list_windows()?
                        .into_iter()
                        .find(|w| w.id == window.id)
                        .ok_or(crate::i18n::text("対象ウインドウが閉じられました", "The target window was closed"))?;
                    let (x, y) = relative_point(*x, *y, &saved.recording, &current)?;
                    without_popup(app, || {
                        send(rdev::EventType::MouseMove { x, y })?;
                        send(rdev::EventType::Wheel {
                            delta_x: *dx,
                            delta_y: *dy,
                        })?;
                        Ok(())
                    })?;
                }
            }
            let shorten_waits = app.state::<PlaybackState>().2.lock().map_err(|e| e.to_string())?.shorten_waits;
            if !matches!(event, RecordedEvent::Click { .. } | RecordedEvent::Screenshot { .. }) {
                let wait = post_operation_wait(event, shorten_waits);
                countdown(app, wait, &check)?;
                action_settled.set(wait > 0);
            }
            progress(app, |value| {
                value.completed.push(describe(event));
                if matches!(event, RecordedEvent::Screenshot { .. }) {
                    value.captures += 1;
                }
            });
            Ok(())
        },
    )?;
    apply_captured_cursor_modes(&mut saved.recording.events, captured_cursor_modes);
    if pictures.is_empty() {
        check()?;
        let shorten_waits = app.state::<PlaybackState>().2.lock().map_err(|e| e.to_string())?.shorten_waits;
        progress(app, |value| {
            value.message = crate::i18n::text("キャプチャ：現在のウィンドウを新しく撮影", "Capture: take a new screenshot of the current window").into()
        });
        pictures.push(without_popup(app, || {
            countdown(app, if shorten_waits { if action_settled.get() { 0 } else { 1000 } } else { 250 }, &check)?;
            wait_for_visual_ready(app, window, None, &check)?;
            let include_cursor = app.state::<PlaybackState>().2.lock().map_err(|e| e.to_string())?.include_cursor;
            if include_cursor {
                let pointer = crate::cursor_capture::current_pointer(&window.id)?.map(|pointer|
                    pointer_in_recording_space(pointer, saved.recording.bounds,
                        [window.x as f64, window.y as f64, window.width as f64, window.height as f64]));
                let png = crate::commands::attach_window_snap_metadata(crate::cursor_capture::snapshot_window(&window.id)?.raw_png, &window.id)?;
                let elapsed_ms = saved.recording.events.last().map(event_elapsed_ms).unwrap_or(0);
                saved.recording.events.push(RecordedEvent::Screenshot { elapsed_ms, include_cursor: true, pointer });
                Ok(png)
            } else { snapshot(&window.id) }
        })?);
    }
    save_snapshots(&saved, pictures)
}

fn replay_fullscreen(app: &tauri::AppHandle, mut saved: SavedRecording, area: crate::cursor_capture::CursorCaptureTarget, max_wait_ms: u64) -> Result<Vec<metadata::LoadedImageResult>, String> {
    if !matches!(area, crate::cursor_capture::CursorCaptureTarget::Fullscreen) || saved.recording.events.is_empty() || saved.recording.events.iter().any(|event| !matches!(event, RecordedEvent::Screenshot { .. })) {
        return Err(crate::i18n::text("全画面の再撮影に不正な操作が含まれています", "The fullscreen recording contains invalid operations").into());
    }
    if let Some(main) = app.get_webview_window("main") { main.hide().map_err(|e| e.to_string())?; }
    let check = || -> Result<(), String> {
        if app.state::<PlaybackState>().1.load(Ordering::SeqCst) { Err(crate::i18n::text("再生を停止しました", "Playback stopped").into()) } else { Ok(()) }
    };
    let mut pictures = Vec::new();
    let mut captured_cursor_modes = Vec::new();
    play_events(&saved.recording.events, max_wait_ms,
        |_, index| {
            wait_for_step(app)?;
            let delay = {
                let state = app.state::<PlaybackState>();
                let progress = state.2.lock().map_err(|e| e.to_string())?;
                operation_wait(progress.waits_ms[index], progress.shorten_waits, true)
            };
            progress(app, |value| { value.step = index + 1; value.message = describe(&saved.recording.events[index]); });
            countdown(app, delay, &check)?;
            check()
        },
        |event| {
            let RecordedEvent::Screenshot { pointer, include_cursor, .. } = event else { unreachable!() };
            let include_cursor = app.state::<PlaybackState>().2.lock().map_err(|e| e.to_string())?
                .cursor_override.unwrap_or(*include_cursor);
            let shorten_waits = app.state::<PlaybackState>().2.lock().map_err(|e| e.to_string())?.shorten_waits;
            let bounds = area.bounds()?;
            let point = pointer.as_ref().map(|pointer| relative_point(pointer.x, pointer.y, &saved.recording, &bounds)).transpose()?;
            pictures.push(without_popup(app, || {
                check()?;
                if let Some((x, y)) = point {
                    send(rdev::EventType::MouseMove { x, y })?;
                }
                wait_before_screenshot(app, shorten_waits, point.is_some(), false, &check)?;
                wait_for_visual_ready(app, &bounds, None, &check)?;
                let recorded_pointer = if include_cursor && pointer.is_none() {
                    crate::cursor_capture::pointer_in_bounds(&bounds)?.map(|pointer|
                        pointer_in_recording_space(pointer, saved.recording.bounds,
                            [bounds.x as f64, bounds.y as f64, bounds.width as f64, bounds.height as f64]))
                } else { pointer.clone() };
                let png = crate::cursor_capture::snapshot_target_with_cursor(&area, include_cursor)?.raw_png;
                captured_cursor_modes.push((include_cursor, recorded_pointer));
                Ok(png)
            })?);
            progress(app, |value| { value.completed.push(describe(event)); value.captures += 1; });
            Ok(())
        })?;
    apply_captured_cursor_modes(&mut saved.recording.events, captured_cursor_modes);
    save_snapshots(&saved, pictures)
}

fn event_elapsed_ms(event: &RecordedEvent) -> u64 {
    match event {
        RecordedEvent::Click { elapsed_ms, .. } | RecordedEvent::Text { elapsed_ms, .. } |
        RecordedEvent::Key { elapsed_ms, .. } | RecordedEvent::Scroll { elapsed_ms, .. } |
        RecordedEvent::Screenshot { elapsed_ms, .. } => *elapsed_ms,
    }
}

fn point_in_bounds(x: f64, y: f64, [left, top, width, height]: [f64; 4]) -> bool {
    width > 0.0 && height > 0.0 && x >= left && y >= top && x < left + width && y < top + height
}

fn pointer_in_recording_space(mut pointer: markits::recording::RecordedPointer, original: [i64; 4], current: [f64; 4]) -> markits::recording::RecordedPointer {
    if current[2] > 0.0 && current[3] > 0.0 {
        pointer.x = original[0] as f64 + (pointer.x - current[0]) * original[2] as f64 / current[2];
        pointer.y = original[1] as f64 + (pointer.y - current[1]) * original[3] as f64 / current[3];
    }
    pointer
}

fn apply_captured_cursor_modes(events: &mut [RecordedEvent], modes: Vec<(bool, Option<markits::recording::RecordedPointer>)>) {
    let mut modes = modes.into_iter();
    for event in events {
        if let RecordedEvent::Screenshot { include_cursor, pointer, .. } = event {
            if let Some((enabled, position)) = modes.next() { *include_cursor = enabled; *pointer = position; }
        }
    }
}

fn wait_before_screenshot(app: &tauri::AppHandle, shorten_waits: bool, moved_pointer: bool, action_settled: bool, check: &impl Fn() -> Result<(), String>) -> Result<(), String> {
    let delay = screenshot_settle_wait(shorten_waits, moved_pointer, action_settled);
    if delay == 1000 {
        progress(app, |value| value.message = crate::i18n::text(
            "画面の切り替わりを待っています（撮影前に1秒）",
            "Waiting for the screen transition (1 second before capture)").into());
    }
    countdown(app, delay, check)
}

fn frame_change_fraction(before: &image::GrayImage, after: &image::GrayImage) -> f64 {
    if before.dimensions() != after.dimensions() { return 1.0; }
    let count = before.as_raw().iter().zip(after.as_raw())
        .filter(|(a, b)| a.abs_diff(**b) > 12).count();
    count as f64 / before.as_raw().len().max(1) as f64
}

struct VisualReadiness {
    baseline: Option<image::GrayImage>,
    previous: image::GrayImage,
    changed: bool,
    stable_since: Duration,
}

impl VisualReadiness {
    fn new(baseline: Option<&image::GrayImage>, initial: image::GrayImage, elapsed: Duration) -> Self {
        let changed = baseline.is_none_or(|before| frame_change_fraction(before, &initial) >= 0.02);
        Self { baseline: baseline.cloned(), previous: initial, changed, stable_since: elapsed }
    }

    fn observe(&mut self, frame: image::GrayImage, elapsed: Duration) -> bool {
        if !self.changed && self.baseline.as_ref().is_some_and(|before| frame_change_fraction(before, &frame) >= 0.02) {
            self.changed = true;
            self.stable_since = elapsed;
        }
        if frame_change_fraction(&self.previous, &frame) > 0.002 { self.stable_since = elapsed; }
        self.previous = frame;
        self.changed && elapsed.saturating_sub(self.stable_since) >= Duration::from_millis(600)
    }
}

fn wait_for_visual_ready(app: &tauri::AppHandle, window: &markits::window_capture::WindowInfo,
    before_navigation: Option<&image::GrayImage>, check: &impl Fn() -> Result<(), String>) -> Result<(), String> {
    let _timing = crate::playback_trace::Span::new("screen.wait_for_transition");
    progress(app, |value| value.message = crate::i18n::text(
        "画面の切り替え・描画の完了を確認しています…",
        "Waiting for the screen transition and drawing to settle…").into());
    let started = std::time::Instant::now();
    let initial = crate::cursor_capture::sample_window_frame(window)?;
    let mut readiness = VisualReadiness::new(before_navigation, initial, started.elapsed());
    loop {
        check()?;
        if started.elapsed() >= Duration::from_secs(10) {
            return Err(crate::i18n::text(
                "画面の切り替え完了を確認できないため撮影を止めました。対象画面を確認して再生してください。",
                "Capture paused because the screen transition could not be confirmed. Check the target and press Play.").into());
        }
        thread::sleep(Duration::from_millis(150));
        let frame = crate::cursor_capture::sample_window_frame(window)?;
        if readiness.observe(frame, started.elapsed()) { return Ok(()); }
    }
}

fn countdown(app: &tauri::AppHandle, mut remaining: u64, check: &impl Fn() -> Result<(), String>) -> Result<(), String> {
    let _timing = crate::playback_trace::Span::with_expected("delay.countdown", Some(remaining));
    progress(app, |value| { value.wait_total_ms = remaining; value.wait_remaining_ms = remaining; });
    while remaining > 0 {
        check()?;
        let chunk = remaining.min(50);
        thread::sleep(Duration::from_millis(chunk));
        remaining -= chunk;
        progress(app, |value| value.wait_remaining_ms = remaining);
    }
    Ok(())
}

fn recorded_waits(events: &[RecordedEvent]) -> Vec<u64> {
    let mut previous = 0;
    events.iter().enumerate().map(|(index, event)| {
        let elapsed = match event {
            RecordedEvent::Click { elapsed_ms, .. } | RecordedEvent::Text { elapsed_ms, .. }
            | RecordedEvent::Key { elapsed_ms, .. } | RecordedEvent::Scroll { elapsed_ms, .. }
            | RecordedEvent::Screenshot { elapsed_ms, .. } => *elapsed_ms,
        };
        let wait = if index == 0 { 0 } else { elapsed.saturating_sub(previous) };
        previous = elapsed;
        wait
    }).collect()
}

fn play_events(
    events: &[RecordedEvent],
    max_wait_ms: u64,
    mut before: impl FnMut(u64, usize) -> Result<(), String>,
    mut action: impl FnMut(&RecordedEvent) -> Result<(), String>,
) -> Result<(), String> {
    let mut previous = 0;
    for (index, event) in events.iter().enumerate() {
        crate::playback_trace::step(index + 1);
        let _step_timing = crate::playback_trace::Span::new("step.total");
        let elapsed = match event {
            RecordedEvent::Click { elapsed_ms, .. }
            | RecordedEvent::Text { elapsed_ms, .. }
            | RecordedEvent::Key { elapsed_ms, .. }
            | RecordedEvent::Scroll { elapsed_ms, .. }
            | RecordedEvent::Screenshot { elapsed_ms, .. } => *elapsed_ms,
        };
        crate::playback_trace::measure("step.prepare", || before(elapsed.saturating_sub(previous).min(max_wait_ms), index))?;
        crate::playback_trace::measure("step.execute", || action(event))?;
        previous = elapsed;
    }
    Ok(())
}

fn relative_point(
    x: f64,
    y: f64,
    recording: &OperationRecording,
    window: &markits::window_capture::WindowInfo,
) -> Result<(f64, f64), String> {
    let [left, top, width, height] = recording.bounds;
    if width <= 0
        || height <= 0
        || !x.is_finite()
        || !y.is_finite()
        || x < left as f64
        || y < top as f64
        || x >= (left + width) as f64
        || y >= (top + height) as f64
    {
        return Err(crate::i18n::text("操作座標が対象ウインドウの外にあります", "The operation coordinates are outside the target window").into());
    }
    Ok((
        window.x as f64 + (x - left as f64) * window.width as f64 / width as f64,
        window.y as f64 + (y - top as f64) * window.height as f64 / height as f64,
    ))
}

fn hover_point(pointer: &markits::recording::RecordedPointer, recording: &OperationRecording,
    window: &markits::window_capture::WindowInfo, elements: &[markits::ui_elements::DetectedUiElement]) -> Result<(f64, f64), String> {
    if let Some(target) = &pointer.target {
        if let Some(element) = target.resolve(elements)? {
            let [x, y] = pointer.target_offset.unwrap_or([0.5, 0.5]);
            if !x.is_finite() || !y.is_finite() || !(0.0..1.0).contains(&x) || !(0.0..1.0).contains(&y) {
                return Err(crate::i18n::text("記録されたポインタ位置が対象部品の外にあります", "The recorded pointer position is outside the target element").into());
            }
            return Ok((element.x + element.width * x, element.y + element.height * y));
        }
    }
    relative_point(pointer.x, pointer.y, recording, window)
}

fn send(event: rdev::EventType) -> Result<(), String> {
    rdev::simulate(&event).map_err(|e| crate::localized!("入力を再生できません: {e:?}", "Cannot replay input: {e:?}"))?;
    thread::sleep(Duration::from_millis(15));
    Ok(())
}

fn chord(keys: &[rdev::Key]) -> Result<(), String> {
    let mut pressed = Vec::new();
    let mut result = Ok(());
    for &key in keys {
        if let Err(error) = send(rdev::EventType::KeyPress(key)) {
            result = Err(error);
            break;
        }
        pressed.push(key);
    }
    for key in pressed.into_iter().rev() {
        let release = send(rdev::EventType::KeyRelease(key));
        if result.is_ok() {
            result = release;
        }
    }
    result
}

fn paste(value: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let previous = clipboard.get_text().ok();
    clipboard
        .set_text(value.to_string())
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let modifier = rdev::Key::MetaLeft;
    #[cfg(not(target_os = "macos"))]
    let modifier = rdev::Key::ControlLeft;
    let result = chord(&[modifier, rdev::Key::KeyV]);
    thread::sleep(Duration::from_millis(150));
    if let Some(previous) = previous {
        let _ = clipboard.set_text(previous);
    }
    result
}

fn parse_chord(value: &str) -> Result<Vec<rdev::Key>, String> {
    use rdev::Key::*;
    value
        .split('+')
        .map(|part| {
            Ok(match part.to_lowercase().as_str() {
                "control" => ControlLeft,
                "alt" => Alt,
                "meta" => MetaLeft,
                "shift" => ShiftLeft,
                "enter" => Return,
                "escape" => Escape,
                "tab" => Tab,
                "backspace" => Backspace,
                "delete" => Delete,
                "space" => Space,
                "up" => UpArrow,
                "down" => DownArrow,
                "left" => LeftArrow,
                "right" => RightArrow,
                "home" => Home,
                "end" => End,
                "pageup" => PageUp,
                "pagedown" => PageDown,
                "f1" => F1,
                "f2" => F2,
                "f3" => F3,
                "f4" => F4,
                "f5" => F5,
                "f6" => F6,
                "f7" => F7,
                "f8" => F8,
                "f9" => F9,
                "f10" => F10,
                "f11" => F11,
                "f12" => F12,
                "a" => KeyA,
                "b" => KeyB,
                "c" => KeyC,
                "d" => KeyD,
                "e" => KeyE,
                "f" => KeyF,
                "g" => KeyG,
                "h" => KeyH,
                "i" => KeyI,
                "j" => KeyJ,
                "k" => KeyK,
                "l" => KeyL,
                "m" => KeyM,
                "n" => KeyN,
                "o" => KeyO,
                "p" => KeyP,
                "q" => KeyQ,
                "r" => KeyR,
                "s" => KeyS,
                "t" => KeyT,
                "u" => KeyU,
                "v" => KeyV,
                "w" => KeyW,
                "x" => KeyX,
                "y" => KeyY,
                "z" => KeyZ,
                "0" => Num0,
                "1" => Num1,
                "2" => Num2,
                "3" => Num3,
                "4" => Num4,
                "5" => Num5,
                "6" => Num6,
                "7" => Num7,
                "8" => Num8,
                "9" => Num9,
                _ => return Err(crate::localized!("未対応のキー操作です: {value}", "Unsupported key operation: {value}")),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_override_positions_are_normalized_and_legacy_frames_are_repaired() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::new(16, 16).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let saved = SavedRecording { capture_area: None, window_id: "0x123".into(), pid: 42,
            recording: OperationRecording { version: 1, window: "Test".into(), bounds: [1894, 153, 1467, 1019],
                events: vec![RecordedEvent::Screenshot { elapsed_ms: 0, include_cursor: true,
                    pointer: Some(markits::recording::RecordedPointer { x: 812.0, y: 447.0, target: None, target_offset: None }) }] } };
        let bytes = metadata::embed_text_chunk(png.get_ref(), OPERATIONS_KEY, &serde_json::to_string(&saved).unwrap()).unwrap();
        let bytes = metadata::embed_text_chunk(&bytes, metadata::MARKITS_CAPTURE_TARGET_KEYWORD,
            r#"{"role":"window","x":345,"y":136,"width":1467,"height":1019}"#).unwrap();
        let reloaded = extract(&bytes).unwrap().unwrap();
        let RecordedEvent::Screenshot { pointer: Some(pointer), .. } = &reloaded.recording.events[0] else { panic!("missing pointer") };
        assert_eq!((pointer.x, pointer.y), (2361.0, 464.0));
        let current = markits::window_capture::WindowInfo { id: "0x123".into(), title: "Test".into(), x: 345, y: 136, width: 1467, height: 1019 };
        assert_eq!(relative_point(pointer.x, pointer.y, &reloaded.recording, &current).unwrap(), (812.0, 447.0));
        let fresh = pointer_in_recording_space(markits::recording::RecordedPointer { x: 812.0, y: 447.0, target: None, target_offset: None },
            saved.recording.bounds, [345.0, 136.0, 1467.0, 1019.0]);
        assert_eq!((fresh.x, fresh.y), (2361.0, 464.0));
    }

    #[test]
    fn transition_wait_does_not_accept_the_unchanged_old_screen() {
        let old = image::GrayImage::from_pixel(96, 64, image::Luma([20]));
        let new = image::GrayImage::from_pixel(96, 64, image::Luma([160]));
        let mut readiness = VisualReadiness::new(Some(&old), old.clone(), Duration::ZERO);
        assert!(!readiness.observe(old.clone(), Duration::from_secs(1)));
        assert!(!readiness.observe(old.clone(), Duration::from_secs(5)));
        assert!(!readiness.observe(new.clone(), Duration::from_millis(5200)));
        assert!(!readiness.observe(new.clone(), Duration::from_millis(5500)));
        assert!(readiness.observe(new, Duration::from_millis(5800)));
    }

    #[test]
    fn transition_wait_restarts_its_settle_period_during_animation() {
        let a = image::GrayImage::from_pixel(96, 64, image::Luma([20]));
        let b = image::GrayImage::from_pixel(96, 64, image::Luma([80]));
        let c = image::GrayImage::from_pixel(96, 64, image::Luma([160]));
        let mut readiness = VisualReadiness::new(None, a, Duration::ZERO);
        assert!(!readiness.observe(b, Duration::from_millis(500)));
        assert!(!readiness.observe(c.clone(), Duration::from_millis(900)));
        assert!(!readiness.observe(c.clone(), Duration::from_millis(1200)));
        assert!(readiness.observe(c, Duration::from_millis(1500)));
    }

    #[test]
    fn recapture_persists_each_actual_cursor_mode_and_keeps_hover_positions() {
        let original: Vec<RecordedEvent> = serde_json::from_str(r#"[
            {"kind":"click","elapsed_ms":0,"x":10,"y":20},
            {"kind":"screenshot","elapsed_ms":100,"include_cursor":true,"pointer":{"x":10,"y":20}},
            {"kind":"screenshot","elapsed_ms":200,"include_cursor":false}
        ]"#).unwrap();
        let mut events = original.clone();
        let pointer = markits::recording::RecordedPointer { x: 10.0, y: 20.0, target: None, target_offset: None };
        apply_captured_cursor_modes(&mut events, vec![(false, Some(pointer.clone())), (true, Some(pointer))]);
        assert!(matches!(events[0], RecordedEvent::Click { elapsed_ms: 0, .. }));
        assert!(matches!(&events[1], RecordedEvent::Screenshot { include_cursor: false, pointer: Some(p), .. } if p.x == 10.0));
        assert!(matches!(&events[2], RecordedEvent::Screenshot { include_cursor: true, pointer: Some(p), .. } if p.y == 20.0));
        let reloaded: Vec<RecordedEvent> = serde_json::from_str(&serde_json::to_string(&events).unwrap()).unwrap();
        assert!(matches!(reloaded[1], RecordedEvent::Screenshot { include_cursor: false, .. }));
        assert!(matches!(reloaded[2], RecordedEvent::Screenshot { include_cursor: true, .. }));
    }

    #[test]
    fn shortened_mode_waits_one_second_per_operation_without_double_waiting_for_screenshots() {
        let click = RecordedEvent::Click { elapsed_ms: 0, x: 0.0, y: 0.0, target: None, visual: None };
        let screenshot = RecordedEvent::Screenshot { elapsed_ms: 1, include_cursor: false, pointer: None };
        assert_eq!(operation_wait(60_000, true, false), 0);
        assert_eq!(post_operation_wait(&click, true), 1000);
        assert_eq!(post_operation_wait(&click, false), 1000);
        assert_eq!(screenshot_settle_wait(true, false, true), 0);
        assert_eq!(post_operation_wait(&screenshot, true), 0);
        assert_eq!(screenshot_settle_wait(true, false, false), 1000);
        assert_eq!(screenshot_settle_wait(true, true, false), 1000);
        assert!(PlaybackState::default().2.lock().unwrap().shorten_waits);
    }

    #[test]
    fn shortened_playback_skips_recorded_delays_including_the_capture_gap() {
        let events: Vec<RecordedEvent> = serde_json::from_str(r#"[
            {"kind":"click","elapsed_ms":0,"x":10,"y":20},
            {"kind":"click","elapsed_ms":2048,"x":10,"y":20},
            {"kind":"screenshot","elapsed_ms":15806}
        ]"#).unwrap();
        let configured = recorded_waits(&events);
        assert_eq!(configured, vec![0, 2048, 13758]);
        let mut waits = Vec::new();
        let mut executed = 0;
        play_events(&events, u64::MAX, |_, index| {
            waits.push(operation_wait(configured[index], true,
                matches!(events[index], RecordedEvent::Screenshot { .. })));
            Ok(())
        }, |_| { executed += 1; Ok(()) }).unwrap();
        assert_eq!(waits, vec![0, 0, 0]);
        assert_eq!(executed, 3);
    }

    #[test]
    fn recorded_mode_keeps_user_delays_and_existing_pointer_settling() {
        assert_eq!(operation_wait(3456, false, false), 3456);
        assert_eq!(operation_wait(0, false, false), 0);
        assert_eq!(screenshot_settle_wait(false, false, false), 0);
        assert_eq!(screenshot_settle_wait(false, true, false), 300);
    }

    #[test]
    fn pointer_replay_uses_named_control_offset_then_recorded_window_coordinates() {
        let recording = OperationRecording { version: 1, window: "Test".into(), bounds: [100, 100, 200, 100], events: vec![] };
        let window = markits::window_capture::WindowInfo { id: "1".into(), title: "Test".into(), x: 300, y: 400, width: 400, height: 200 };
        let pointer = markits::recording::RecordedPointer {
            x: 150.0, y: 125.0,
            target: Some(markits::semantic_target::Target { role: "button".into(), name: "Save".into(), reveals: None }),
            target_offset: Some([0.25, 0.75]),
        };
        let element = markits::ui_elements::DetectedUiElement { role: "button".into(), name: Some("Save".into()),
            x: 600.0, y: 700.0, width: 80.0, height: 40.0, pid: None, window_id: None };
        assert_eq!(hover_point(&pointer, &recording, &window, &[element.clone()]).unwrap(), (620.0, 730.0));
        assert_eq!(hover_point(&pointer, &recording, &window, &[]).unwrap(), (400.0, 450.0));
        assert!(hover_point(&pointer, &recording, &window, &[element.clone(), element]).is_err());
    }

    #[test]
    fn pointer_capture_keeps_hover_target_and_mode_in_saved_png() {
        let pointer = markits::recording::RecordedPointer { x: 20.0, y: 30.0,
            target: Some(markits::semantic_target::Target { role: "button".into(), name: "Save".into(), reveals: None }),
            target_offset: Some([0.2, 0.3]) };
        let saved = SavedRecording { capture_area: None, recording: OperationRecording { version: 1, window: "Test".into(),
            bounds: [0, 0, 100, 100], events: vec![RecordedEvent::Screenshot { elapsed_ms: 10, include_cursor: true, pointer: Some(pointer) }] },
            window_id: "0x123".into(), pid: 42 };
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::new(16, 16).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = metadata::embed_text_chunk(png.get_ref(), OPERATIONS_KEY, &serde_json::to_string(&saved).unwrap()).unwrap();
        let reloaded = extract(&png).unwrap().unwrap();
        let RecordedEvent::Screenshot { include_cursor, pointer: Some(pointer), .. } = &reloaded.recording.events[0] else { panic!("missing pointer capture") };
        assert!(*include_cursor);
        assert_eq!((pointer.x, pointer.y), (20.0, 30.0));
        assert_eq!(pointer.target.as_ref().unwrap().name, "Save");
        assert_eq!(pointer.target_offset, Some([0.2, 0.3]));
        let old: RecordedEvent = serde_json::from_str(r#"{"kind":"screenshot","elapsed_ms":10}"#).unwrap();
        assert!(matches!(old, RecordedEvent::Screenshot { include_cursor: false, pointer: None, .. }));
    }

    #[test]
    fn playback_wait_cap_preserves_short_intervals_and_all_operations() {
        let events = vec![RecordedEvent::Screenshot { elapsed_ms: 50, include_cursor: false, pointer: None },
            RecordedEvent::Screenshot { elapsed_ms: 10050, include_cursor: false, pointer: None },
            RecordedEvent::Screenshot { elapsed_ms: 10100, include_cursor: false, pointer: None }];
        for (limit, expected) in [(300, vec![50, 300, 50]), (0, vec![0, 0, 0])] {
            let mut waits = Vec::new();
            let mut count = 0;
            play_events(&events, limit, |delay, _| { waits.push(delay); Ok(()) },
                |_| { count += 1; Ok(()) }).unwrap();
            assert_eq!(waits, expected);
            assert_eq!(count, 3);
        }
    }

    #[test]
    fn capture_window_identity_survives_export_and_editable_source_reload() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::new(16, 16).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let identity = r#"{"window_id":"hwnd:0x123","pid":42,"name":"Test"}"#;
        let source = metadata::embed_text_chunk(png.get_ref(), metadata::MARKITS_CAPTURE_TARGET_KEYWORD, identity).unwrap();
        let exported = copy_metadata(png.get_ref(), &source).unwrap();
        assert_eq!(metadata::extract_text_chunk(&exported, metadata::MARKITS_CAPTURE_TARGET_KEYWORD).unwrap().as_deref(), Some(identity));
        let source_url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png.get_ref()));
        let exported = metadata::embed_text_chunk(&exported, metadata::MARKITS_SOURCE_KEYWORD, &source_url).unwrap();
        let exported = metadata::embed_annotations(&exported, r#"{"canvas":{"width":16,"height":16},"annotations":[]}"#).unwrap();
        let loaded = metadata::load_image_with_metadata(&exported).unwrap();
        let restored = decode(&loaded.image_data_url).unwrap();
        assert_eq!(metadata::extract_text_chunk(&restored, metadata::MARKITS_CAPTURE_TARGET_KEYWORD).unwrap().as_deref(), Some(identity));
    }

    #[test]
    fn replay_captures_intermediate_states_instead_of_only_the_final_state() {
        let events = vec![
            RecordedEvent::Text {
                elapsed_ms: 10,
                value: "first".into(),
            },
            RecordedEvent::Screenshot { elapsed_ms: 30, include_cursor: false, pointer: None },
            RecordedEvent::Text {
                elapsed_ms: 60,
                value: "second".into(),
            },
            RecordedEvent::Screenshot { elapsed_ms: 90, include_cursor: false, pointer: None },
        ];
        let mut state = String::new();
        let mut pictures = Vec::new();
        let mut waits = Vec::new();
        play_events(
            &events,
            u64::MAX,
            |delay, _| {
                waits.push(delay);
                Ok(())
            },
            |event| {
                match event {
                    RecordedEvent::Text { value, .. } => state = value.clone(),
                    RecordedEvent::Screenshot { .. } => pictures.push(state.clone()),
                    _ => unreachable!(),
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(pictures, ["first", "second"]);
        assert_eq!(waits, [10, 20, 30, 30]);
    }

    #[test]
    fn replay_stops_at_a_failed_step_without_taking_later_screenshots() {
        let events = vec![
            RecordedEvent::Screenshot { elapsed_ms: 0, include_cursor: false, pointer: None },
            RecordedEvent::Screenshot { elapsed_ms: 1, include_cursor: false, pointer: None },
        ];
        let mut captures = 0;
        let result = play_events(
            &events,
            u64::MAX,
            |_, index| {
                if index == 1 {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            },
            |_| {
                captures += 1;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(captures, 1);
    }

    #[test]
    fn recording_survives_export_and_reload_with_multiple_capture_points() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::new(16, 16)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let saved = SavedRecording { capture_area: None,
            recording: OperationRecording {
                version: 1,
                window: "Test".into(),
                bounds: [0, 0, 100, 100],
                events: vec![
                    RecordedEvent::Screenshot { elapsed_ms: 10, include_cursor: false, pointer: None },
                    RecordedEvent::Screenshot { elapsed_ms: 20, include_cursor: false, pointer: None },
                ],
            },
            window_id: "0x123".into(),
            pid: 123,
        };
        let source = metadata::embed_text_chunk(
            &png.get_ref(),
            OPERATIONS_KEY,
            &serde_json::to_string(&saved).unwrap(),
        )
        .unwrap();
        let url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(source)
        );
        let exported =
            metadata::embed_text_chunk(&png.get_ref(), metadata::MARKITS_SOURCE_KEYWORD, &url)
                .unwrap();
        let restored = extract(&exported).unwrap().unwrap();
        assert_eq!(restored.recording.events.len(), 2);
        assert_eq!(restored.window_id, "0x123");
        assert!(extract(&png.get_ref()).unwrap().is_none());
    }

    #[test]
    fn multiple_captures_have_distinct_history_items_and_one_thumbnail_group() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::new(16, 16)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let saved = SavedRecording { capture_area: None,
            recording: OperationRecording {
                version: 1,
                window: "Test capture batch".into(),
                bounds: [0, 0, 16, 16],
                events: vec![
                    RecordedEvent::Screenshot { elapsed_ms: 0, include_cursor: false, pointer: None },
                    RecordedEvent::Screenshot { elapsed_ms: 10, include_cursor: false, pointer: None },
                ],
            },
            window_id: "0x123".into(),
            pid: 123,
        };
        let images =
            save_snapshots(&saved, vec![png.get_ref().clone(), png.get_ref().clone()]).unwrap();
        assert_eq!(images.len(), 2);
        assert_ne!(images[0].history_id, images[1].history_id);
        let groups: Vec<_> = images
            .iter()
            .map(|image| {
                let loaded =
                    history::load_history_item(image.history_id.as_ref().unwrap()).unwrap();
                let bytes = decode(&loaded.image_data_url).unwrap();
                assert_eq!(extract(&bytes).unwrap().unwrap().recording.events.len(), 2);
                metadata::extract_text_chunk(&bytes, GROUP_KEY)
                    .unwrap()
                    .unwrap()
            })
            .collect();
        for image in &images {
            history::delete_history_item(image.history_id.as_ref().unwrap()).unwrap();
        }
        assert_eq!(groups[0], groups[1]);
    }

    #[test]
    fn cropped_file_export_excludes_recording_and_editable_background() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::new(16, 16)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let saved = SavedRecording { capture_area: None,
            recording: OperationRecording {
                version: 1,
                window: "Test".into(),
                bounds: [0, 0, 16, 16],
                events: vec![RecordedEvent::Screenshot { elapsed_ms: 10, include_cursor: false, pointer: None }],
            },
            window_id: "0x123".into(),
            pid: 123,
        };
        let original = metadata::embed_text_chunk(
            png.get_ref(),
            OPERATIONS_KEY,
            &serde_json::to_string(&saved).unwrap(),
        )
        .unwrap();
        let cropped_url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png.get_ref())
        );
        let base_url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(original)
        );
        let path = std::env::temp_dir().join(format!(
            "markits-recording-export-{}.png",
            std::process::id()
        ));
        crate::commands::cmd_compose_and_save(
            cropped_url,
            r#"{"canvas":{"width":16,"height":16},"annotations":[]}"#.into(),
            path.to_string_lossy().into(),
            None,
            None,
            None,
            false,
            Some(base_url),
            None,
            None,
        )
        .unwrap();
        let loaded = crate::commands::cmd_load_image(path.to_string_lossy().into()).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(extract(&decode(&loaded.image_data_url).unwrap()).unwrap().is_none());
        assert!(loaded.base_image_data_url.is_none());
    }

    #[test]
    fn moved_window_coordinates_and_keyboard_shortcuts_are_resolved() {
        let recording = OperationRecording {
            version: 1,
            window: "Test".into(),
            bounds: [100, 100, 200, 100],
            events: vec![],
        };
        let window = markits::window_capture::WindowInfo {
            id: "1".into(),
            title: "Test".into(),
            x: 300,
            y: 400,
            width: 400,
            height: 200,
        };
        assert_eq!(
            relative_point(150., 125., &recording, &window).unwrap(),
            (400., 450.)
        );
        assert!(relative_point(50., 125., &recording, &window).is_err());
        assert_eq!(
            parse_chord("Control+Shift+S").unwrap(),
            vec![
                rdev::Key::ControlLeft,
                rdev::Key::ShiftLeft,
                rdev::Key::KeyS
            ]
        );
        assert!(parse_chord("Control+Unsupported").is_err());
    }
}
