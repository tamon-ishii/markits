use markits::recording::{OperationRecording, RecordingSession, RecordingStatus};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default, Clone)]
pub struct RecordingState(
    Arc<Mutex<Option<RecordingSession>>>,
    Arc<Mutex<Vec<RecordingSnapshot>>>,
    Arc<Mutex<Option<std::time::Instant>>>,
);

impl RecordingState {
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
}

#[tauri::command]
pub async fn cmd_recording_screenshot(
    app: AppHandle,
    state: State<'_, RecordingState>,
) -> Result<String, String> {
    let shared = state.inner().clone();
    let exclude = control_bounds(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = shared.0.lock().map_err(|e| e.to_string())?;
        let session = guard.as_mut().ok_or("No recording is active")?;
        let status = session.status(exclude)?;
        session.pause(true)?;
        let captured = crate::playback::snapshot(session.window_id());
        let resume = session.pause(status.paused);
        let png = captured?;
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
            });
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
pub async fn cmd_recording_start(
    app: AppHandle,
    state: State<'_, RecordingState>,
    window_id: String,
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
        *shared.2.lock().map_err(|e| e.to_string())? = Some(std::time::Instant::now());
        let session = RecordingSession::start(
            &window_id,
            &std::env::current_exe().map_err(|e| e.to_string())?,
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
    .inner_size(440.0, 420.0)
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
            snapshots.push(RecordingSnapshot {
                index: recording.events.len(),
                elapsed_ms,
                png: crate::playback::snapshot(&window_id)?,
            });
        }
        insert_screenshots(&mut recording.events, &snapshots);
        let pictures = snapshots.into_iter().map(|snapshot| snapshot.png).collect();
        let saved = crate::playback::SavedRecording {
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
        | Screenshot { elapsed_ms } => *elapsed_ms,
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
            },
            RecordingSnapshot {
                index: 1,
                elapsed_ms: 250,
                png: vec![],
            },
            RecordingSnapshot {
                index: 2,
                elapsed_ms: 500,
                png: vec![],
            },
        ];
        insert_screenshots(&mut events, &frames);
        assert_eq!(
            events.iter().map(event_time).collect::<Vec<_>>(),
            [100, 200, 250, 300, 500]
        );
        assert!(matches!(events[1], RecordedEvent::Screenshot { .. }));
        assert!(matches!(events[2], RecordedEvent::Screenshot { .. }));
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
