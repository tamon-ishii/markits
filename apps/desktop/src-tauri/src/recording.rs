use markits::recording::{OperationRecording, RecordingSession, RecordingStatus};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

#[derive(Default, Clone)]
pub struct RecordingState(Arc<Mutex<Option<RecordingSession>>>);

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
            if let Some(main) = host.get_webview_window("main") {
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
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_mut()
        .ok_or("No recording is active")?
        .status(control_bounds(&app))
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
    tauri::async_runtime::spawn_blocking(move || session.finish(exclude))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn cmd_recording_cancel(
    app: AppHandle,
    state: State<'_, RecordingState>,
) -> Result<(), String> {
    state.0.lock().map_err(|e| e.to_string())?.take();
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
