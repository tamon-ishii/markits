use markits::recording::{OperationRecording, RecordingSession, RecordingStatus};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};
use base64::Engine;

#[derive(Default, Clone)]
pub struct RecordingState(
    Arc<Mutex<Option<RecordingSession>>>,
    Arc<Mutex<Vec<RecordingSnapshot>>>,
    Arc<Mutex<Option<std::time::Instant>>>,
    Arc<AtomicBool>,
);

impl RecordingState {
    pub fn is_active(&self) -> bool {
        self.0.lock().map(|session| session.is_some()).unwrap_or(false)
    }
    pub fn claim_playback(&self, playback: &crate::playback::PlaybackState) -> Result<(), String> {
        let session = self.0.lock().map_err(|e| e.to_string())?;
        if session.is_some() {
            return Err("記録中は再生できません".into());
        }
        if playback.0.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err("操作を再生中です".into());
        }
        Ok(())
    }
}

struct RecordingSnapshot {
    index: usize,
    elapsed_ms: u64,
    png: Vec<u8>,
    include_cursor: bool,
    pointer: Option<markits::recording::RecordedPointer>,
}

#[tauri::command]
pub fn cmd_recording_cursor_mode(state: State<'_, RecordingState>, enabled: Option<bool>) -> Result<bool, String> {
    if let Some(enabled) = enabled {
        if enabled && !cfg!(target_os = "windows") { return Err("ポインタ付き撮影はWindows専用です".into()); }
        state.3.store(enabled, Ordering::SeqCst);
    }
    Ok(state.3.load(Ordering::SeqCst))
}

fn capture_frame(id: &str, include_cursor: bool) -> Result<(Vec<u8>, Option<markits::recording::RecordedPointer>), String> {
    if include_cursor {
        let mut pointer = crate::cursor_capture::current_pointer(id)?;
        let captured = crate::cursor_capture::snapshot_window(id)?;
        #[cfg(target_os = "windows")]
        if let Some(pointer) = &mut pointer {
            if let Some((target, [left, top, right, bottom])) =
                markits::window_capture::pointer_targets_window(id, pointer.x, pointer.y).unwrap_or(false)
                    .then(|| markits::recording::named_target_at_point(pointer.x as i32, pointer.y as i32)).flatten() {
                if right > left && bottom > top {
                    pointer.target_offset = Some([(pointer.x - left) / (right - left), (pointer.y - top) / (bottom - top)]);
                    pointer.target = Some(target);
                }
            }
        }
        Ok((captured.raw_png, pointer))
    } else { Ok((crate::playback::snapshot(id)?, None)) }
}

pub(crate) fn single_pointer_capture(id: &str) -> Result<String, String> {
    let window = markits::window_capture::list_windows()?.into_iter().find(|w| w.id == id)
        .ok_or("撮影対象のウィンドウが見つかりません")?;
    let pid = markits::window_capture::window_process_ids()?.into_iter()
        .find(|(candidate, _)| candidate == id).ok_or("撮影対象のアプリが見つかりません")?.1;
    let (png, pointer) = capture_frame(id, true)?;
    let saved = crate::playback::SavedRecording { capture_area: None,
        recording: OperationRecording {
            version: 1, window: window.title,
            bounds: [window.x as i64, window.y as i64, window.width as i64, window.height as i64],
            events: vec![markits::recording::RecordedEvent::Screenshot { elapsed_ms: 0, include_cursor: true, pointer }],
        },
        window_id: id.into(), pid,
    };
    let png = crate::metadata::embed_text_chunk(&png, crate::playback::OPERATIONS_KEY,
        &serde_json::to_string(&saved).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

#[tauri::command]
pub async fn cmd_recording_screenshot(
    app: AppHandle,
    state: State<'_, RecordingState>,
) -> Result<String, String> {
    let shared = state.inner().clone();
    let host = app.clone();
    let exclude = control_bounds(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = shared.0.lock().map_err(|e| e.to_string())?;
        let session = guard.as_mut().ok_or("No recording is active")?;
        let status = session.status(exclude)?;
        session.pause(true)?;
        let control = host.get_webview_window("recording");
        if let Some(control) = &control {
            let _ = control.hide();
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        let include_cursor = shared.3.load(Ordering::SeqCst);
        let captured = capture_frame(session.window_id(), include_cursor);
        if let Some(control) = control { let _ = control.show(); }
        let resume = session.pause(status.paused);
        let (png, pointer) = captured?;
        resume?;
        let thumbnail =
            crate::history::generate_thumbnail_data_url(&png, 200).map_err(|e| e.to_string())?;
        let elapsed_ms = shared
            .2
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .map_or(0, |start| start.elapsed().as_millis() as u64);
        shared
            .1
            .lock()
            .map_err(|e| e.to_string())?
            .push(RecordingSnapshot {
                index: status.events.len(),
                elapsed_ms,
                png,
                include_cursor,
                pointer,
            });
        let _ = host.emit_to("recording", "recording-screenshot", &thumbnail);
        Ok(thumbnail)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn cmd_recording_windows() -> Result<Vec<markits::window_capture::WindowInfo>, String> {
    tauri::async_runtime::spawn_blocking(markits::window_capture::list_windows)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn cmd_recording_start_from_image(app: AppHandle, image_data_url: String) -> Result<(), String> {
    let payload = image_data_url.split_once(',').ok_or("画像データが不正です")?.1;
    let bytes = base64::engine::general_purpose::STANDARD.decode(payload).map_err(|e| e.to_string())?;
    let (id, pid) = if let Some(saved) = crate::playback::extract(&bytes)? {
        if saved.capture_area.is_some() {
            return Err("全画面の画像には単一の撮影元ウィンドウがありません。ホームの「操作を記録」から対象を選択してください。".into());
        }
        (saved.window_id, Some(saved.pid))
    } else {
        let json = crate::metadata::extract_text_chunk(&bytes, crate::metadata::MARKITS_CAPTURE_TARGET_KEYWORD)
            .map_err(|e| e.to_string())?
            .ok_or("この画像には撮影元ウィンドウの情報がありません。新しくウィンドウを撮影してから記録してください。")?;
        let target: crate::ui_elements::DetectedUiElement = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        let identity = target.window_id.as_deref().ok_or("撮影元ウィンドウのIDがありません")?;
        let native = if let Some(handle) = identity.strip_prefix("hwnd:") {
            let handle = u64::from_str_radix(handle.trim_start_matches("0x"), 16).map_err(|e| e.to_string())?;
            format!("0x{:x}", handle as u32)
        } else if identity.starts_with("0x") { identity.to_owned() }
        else { format!("0x{:x}", identity.parse::<u64>().map_err(|e| e.to_string())?) };
        (native, target.pid)
    };
    let owners = markits::window_capture::window_process_ids()?;
    if !owners.iter().any(|(candidate, owner)| candidate == &id && pid.is_none_or(|pid| *owner == pid)) {
        return Err("撮影元ウィンドウが閉じられています。対象を撮影し直してください。".into());
    }
    cmd_recording_start(app.clone(), app.state(), id, None).await
}

#[tauri::command]
pub async fn cmd_recording_start(
    app: AppHandle,
    state: State<'_, RecordingState>,
    window_id: String,
    include_cursor: Option<bool>,
) -> Result<(), String> {

    let shared = state.inner().clone();
    let host = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = shared.0.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Err("A recording is already active".into());
        }
        if host
            .state::<crate::playback::PlaybackState>()
            .0
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err("再生中は記録を開始できません".into());
        }
        shared.1.lock().map_err(|e| e.to_string())?.clear();
        shared.3.store(include_cursor.unwrap_or(false) && cfg!(target_os = "windows"), Ordering::SeqCst);
        *shared.2.lock().map_err(|e| e.to_string())? = Some(std::time::Instant::now());
        let shortcut = host.state::<crate::AppState>().shortcut.lock().map_err(|e| e.to_string())?.clone();
        let session = RecordingSession::start_with_capture_shortcut(
            &window_id,
            &std::env::current_exe().map_err(|e| e.to_string())?,
            &shortcut,
        )?;
        *guard = Some(session);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    let control = tauri::WebviewWindowBuilder::new(
        &app,
        "recording",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Markits — 操作を記録中")
    .inner_size(480.0, 760.0)
    .always_on_top(true)
    .focused(false)
    .build();
    match control {
        Ok(_) => {
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.hide();
            }
            Ok(())
        }
        Err(error) => {
            let _ = cmd_recording_cancel(app, state);
            Err(error.to_string())
        }
    }
}

fn control_bounds(app: &AppHandle) -> Option<[f64; 4]> {
    let window = app.get_webview_window("recording")?;
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    #[cfg(target_os = "macos")]
    let scale = window.scale_factor().ok()?;
    #[cfg(not(target_os = "macos"))]
    let scale = 1.0;
    Some([
        position.x as f64 / scale,
        position.y as f64 / scale,
        size.width as f64 / scale,
        size.height as f64 / scale,
    ])
}

#[tauri::command]
pub fn cmd_recording_status(
    app: AppHandle,
    state: State<'_, RecordingState>,
) -> Result<RecordingStatus, String> {
    let mut status = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_mut()
        .ok_or("No recording is active")?
        .status(control_bounds(&app))?;
    insert_screenshots(
        &mut status.events,
        &state.1.lock().map_err(|e| e.to_string())?,
    );
    Ok(status)
}

#[tauri::command]
pub fn cmd_recording_pause(state: State<'_, RecordingState>, paused: bool) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .ok_or("No recording is active")?
        .pause(paused)
}

#[tauri::command]
pub async fn cmd_recording_finish(
    app: AppHandle,
    state: State<'_, RecordingState>,
) -> Result<OperationRecording, String> {
    let session = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("No recording is active")?;
    let exclude = control_bounds(&app);
    let window_id = session.window_id().to_string();
    let elapsed_ms = state
        .2
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .map_or(0, |start| start.elapsed().as_millis() as u64);
    let snapshots = std::mem::take(&mut *state.1.lock().map_err(|e| e.to_string())?);
    let host = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let pid = markits::window_capture::window_process_ids()?
            .into_iter()
            .find(|(id, _)| id == &window_id)
            .ok_or("対象ウインドウが閉じられました")?
            .1;
        let mut recording = session.finish(exclude)?;
        let mut snapshots = snapshots;
        if snapshots.is_empty() {
            let include_cursor = host.state::<RecordingState>().3.load(Ordering::SeqCst);
            let (png, pointer) = capture_frame(&window_id, include_cursor)?;
            snapshots.push(RecordingSnapshot {
                index: recording.events.len(),
                elapsed_ms,
                png,
                include_cursor,
                pointer,
            });
        }
        insert_screenshots(&mut recording.events, &snapshots);
        let pictures = snapshots.into_iter().map(|snapshot| snapshot.png).collect();
        let saved = crate::playback::SavedRecording { capture_area: None,
            recording: recording.clone(),
            window_id,
            pid,
        };
        let images = crate::playback::save_snapshots(&saved, pictures)?;
        if let Some(image) = images.last() {
            host.emit("capture-finished", image)
                .map_err(|e| e.to_string())?;
        }
        Ok(recording)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn event_time(event: &markits::recording::RecordedEvent) -> u64 {
    use markits::recording::RecordedEvent::*;
    match event {
        Click { elapsed_ms, .. }
        | Text { elapsed_ms, .. }
        | Key { elapsed_ms, .. }
        | Scroll { elapsed_ms, .. }
        | Screenshot { elapsed_ms, .. } => *elapsed_ms,
    }
}

fn insert_screenshots(
    events: &mut Vec<markits::recording::RecordedEvent>,
    snapshots: &[RecordingSnapshot],
) {
    for (offset, snapshot) in snapshots.iter().enumerate() {
        let index = (snapshot.index + offset).min(events.len());
        let previous = events
            .get(index.wrapping_sub(1))
            .map(event_time)
            .unwrap_or(0);
        let next = events.get(index).map(event_time).unwrap_or(u64::MAX);
        events.insert(
            index,
            markits::recording::RecordedEvent::Screenshot {
                elapsed_ms: snapshot.elapsed_ms.max(previous).min(next),
                include_cursor: snapshot.include_cursor,
                pointer: snapshot.pointer.clone(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markits::recording::RecordedEvent;

    #[test]
    fn screenshots_remain_between_the_operations_that_surrounded_each_capture() {
        let mut events = vec![
            RecordedEvent::Text {
                elapsed_ms: 100,
                value: "before".into(),
            },
            RecordedEvent::Text {
                elapsed_ms: 300,
                value: "after".into(),
            },
        ];
        let frames = vec![
            RecordingSnapshot {
                index: 1,
                elapsed_ms: 200,
                png: vec![],
                include_cursor: false,
                pointer: None,
            },
            RecordingSnapshot {
                index: 1,
                elapsed_ms: 250,
                png: vec![],
                include_cursor: true,
                pointer: Some(markits::recording::RecordedPointer { x: 120.0, y: 230.0, target: None, target_offset: None }),
            },
            RecordingSnapshot {
                index: 2,
                elapsed_ms: 500,
                png: vec![],
                include_cursor: false,
                pointer: None,
            },
        ];
        insert_screenshots(&mut events, &frames);
        assert_eq!(
            events.iter().map(event_time).collect::<Vec<_>>(),
            [100, 200, 250, 300, 500]
        );
        assert!(matches!(events[1], RecordedEvent::Screenshot { .. }));
        assert!(matches!(events[2], RecordedEvent::Screenshot { .. }));
        assert!(matches!(&events[2], RecordedEvent::Screenshot { include_cursor: true, pointer: Some(pointer), .. } if pointer.x == 120.0 && pointer.y == 230.0));
        assert!(matches!(events[3], RecordedEvent::Text { .. }));
        assert!(matches!(events[4], RecordedEvent::Screenshot { .. }));
    }
}

#[tauri::command]
pub fn cmd_recording_cancel(
    app: AppHandle,
    state: State<'_, RecordingState>,
) -> Result<(), String> {
    state.0.lock().map_err(|e| e.to_string())?.take();
    state.1.lock().map_err(|e| e.to_string())?.clear();
    if let Some(main) = app.get_webview_window("main") {
        super::commands::force_raise_window(&main);
    }
    if let Some(control) = app.get_webview_window("recording") {
        let _ = control.destroy();
    }
    Ok(())
}

pub fn cancel(app: &AppHandle) {
    let _ = cmd_recording_cancel(app.clone(), app.state::<RecordingState>());
}

pub fn shutdown(app: &AppHandle) {
    if let Ok(mut session) = app.state::<RecordingState>().0.lock() {
        session.take();
    }
}
