use crate::capture;
use crate::history::{self, HistoryItem};
use crate::metadata;
use base64::Engine;
use image::GenericImageView;
use std::collections::HashSet;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::Manager;

use tauri::Emitter;

static DETAIL_SCAN_RUNNING: AtomicBool = AtomicBool::new(false);
static DETAIL_SCAN_FAILURES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

struct DetailScanGuard;

impl Drop for DetailScanGuard {
    fn drop(&mut self) {
        DETAIL_SCAN_RUNNING.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn cmd_capture_screen(app: tauri::AppHandle) -> Result<capture::CapturedImage, String> {
    let window = app.get_webview_window("main");
    if let Some(ref w) = window {
        let _ = w.hide();
    }

    let res = tauri::async_runtime::spawn_blocking(
        || -> Result<capture::CapturedImage, capture::CaptureError> {
            // Wait for OS window manager fade-out/unmap animation to completely finish (e.g. GNOME Mutter ~250ms)
            std::thread::sleep(std::time::Duration::from_millis(350));
            let mut captured = capture::capture_primary_screen()?;
            let elements = crate::ui_elements::capture_desktop_windows(0, 0);
            captured.ui_elements = elements;
            Ok(captured)
        },
    )
    .await;

    if let Some(ref w) = window {
        force_raise_window(w);
    }

    res.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_start_capture(app: tauri::AppHandle) -> Result<(), String> {
    let generation = app
        .state::<crate::AppState>()
        .capture_generation
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    let main_win = app.get_webview_window("main");
    if let Some(ref w) = main_win {
        let _ = w.hide();
    }

    let res = tauri::async_runtime::spawn_blocking(
        || -> Result<capture::CapturedImage, capture::CaptureError> {
            // Wait for OS window manager fade-out/unmap animation to completely finish (e.g. GNOME Mutter ~250ms)
            std::thread::sleep(std::time::Duration::from_millis(350));
            let mut cap = capture::capture_primary_screen()?;
            let elements = crate::ui_elements::capture_desktop_windows(0, 0);
            cap.ui_elements = elements;
            Ok(cap)
        },
    )
    .await;

    if app
        .state::<crate::AppState>()
        .capture_generation
        .load(Ordering::SeqCst)
        != generation
    {
        return Ok(());
    }

    let captured = match res {
        Ok(Ok(captured)) => captured,
        Ok(Err(err)) => {
            if let Some(ref w) = main_win {
                force_raise_window(w);
            }
            return Err(err.to_string());
        }
        Err(err) => {
            if let Some(ref w) = main_win {
                force_raise_window(w);
            }
            return Err(err.to_string());
        }
    };

    if let Some(overlay_win) = app.get_webview_window("overlay") {
        let state = app.state::<crate::AppState>();
        *state.pending_capture.lock().map_err(|e| e.to_string())? = Some(captured);
        show_overlay_window(&overlay_win);
        if *state.overlay_ready.lock().map_err(|e| e.to_string())? {
            if let Some(capture) = state
                .pending_capture
                .lock()
                .map_err(|e| e.to_string())?
                .take()
            {
                overlay_win
                    .emit("show-capture-overlay", &capture)
                    .map_err(|e| e.to_string())?;
            }
        }
    } else {
        if let Some(ref w) = main_win {
            force_raise_window(w);
        }
        return Err("Capture overlay window is unavailable".to_string());
    }

    Ok(())
}

#[tauri::command]
pub fn cmd_overlay_ready(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    *state.overlay_ready.lock().map_err(|e| e.to_string())? = true;
    if let Some(capture) = state
        .pending_capture
        .lock()
        .map_err(|e| e.to_string())?
        .take()
    {
        let overlay = app
            .get_webview_window("overlay")
            .ok_or("Capture overlay window is unavailable")?;
        overlay
            .emit("show-capture-overlay", &capture)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cmd_finish_capture(
    app: tauri::AppHandle,
    raw_data_url: String,
    crop_rect: Option<CropRectInput>,
    ui_elements: Vec<crate::ui_elements::DetectedUiElement>,
) -> Result<(), String> {
    let generation = app
        .state::<crate::AppState>()
        .capture_generation
        .load(Ordering::SeqCst);
    if let Some(overlay_win) = app.get_webview_window("overlay") {
        let _ = overlay_win.hide();
    }
    if let Some(main_win) = app.get_webview_window("main") {
        force_raise_window(&main_win);
    }

    let selected_window = crate::ui_elements::select_window_for_capture(
        &ui_elements,
        crop_rect.as_ref().map(|c| (c.x, c.y, c.width, c.height)),
    );
    let capture_elements = selected_window
        .as_ref()
        .map(|window| vec![window.clone()])
        .unwrap_or(ui_elements);
    if app
        .state::<crate::AppState>()
        .capture_generation
        .load(Ordering::SeqCst)
        != generation
    {
        return Ok(());
    }
    let scan_crop = crop_rect.as_ref().map(|c| (c.x, c.y, c.width, c.height));

    let loaded_res = if let Some(crop) = crop_rect {
        let x = crop.x.max(0.0) as u32;
        let y = crop.y.max(0.0) as u32;
        let w = crop.width.max(1.0) as u32;
        let h = crop.height.max(1.0) as u32;

        let prefix = "data:image/png;base64,";
        let base64_str = if raw_data_url.starts_with(prefix) {
            &raw_data_url[prefix.len()..]
        } else if let Some(pos) = raw_data_url.find(',') {
            &raw_data_url[pos + 1..]
        } else {
            &raw_data_url
        };

        let png_bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .map_err(|e| format!("Base64 decode error: {}", e))?;

        let dynamic_img = image::load_from_memory(&png_bytes).map_err(|e| e.to_string())?;
        let rgba = dynamic_img.to_rgba8();
        let cropped = capture::crop_rgba_image(&rgba, x, y, w, h).map_err(|e| e.to_string())?;
        let captured = capture::rgba_to_captured_image(&cropped).map_err(|e| e.to_string())?;

        let cropped_elements = crate::ui_elements::filter_elements_for_crop(
            &capture_elements,
            x as f64,
            y as f64,
            w as f64,
            h as f64,
        );

        // Keep the snap targets with the capture so history restores them.
        let item = history::save_or_update_history_item(
            None,
            &captured.raw_png,
            None,
            Some(&cropped_elements),
            None,
            None,
        )
        .map_err(|e| e.to_string())?;

        let res = metadata::LoadedImageResult {
            width: captured.width,
            height: captured.height,
            image_data_url: captured.data_url,
            annotations_json: None,
            history_id: Some(item.id.clone()),
            ui_elements: Some(cropped_elements),
            base_image_data_url: None,
            base_width: None,
            base_height: None,
            base_ui_elements: None,
            crop_info: None,
        };

        res
    } else {
        let prefix = "data:image/png;base64,";
        let base64_str = if raw_data_url.starts_with(prefix) {
            &raw_data_url[prefix.len()..]
        } else {
            &raw_data_url
        };
        let png_bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .map_err(|e| e.to_string())?;

        let item = history::save_or_update_history_item(
            None,
            &png_bytes,
            None,
            Some(&capture_elements),
            None,
            None,
        )
        .map_err(|e| e.to_string())?;

        let img = image::load_from_memory(&png_bytes).map_err(|e| e.to_string())?;
        let res = metadata::LoadedImageResult {
            width: img.width(),
            height: img.height(),
            image_data_url: raw_data_url,
            annotations_json: None,
            history_id: Some(item.id.clone()),
            ui_elements: Some(capture_elements),
            base_image_data_url: None,
            base_width: None,
            base_height: None,
            base_ui_elements: None,
            crop_info: None,
        };
        res
    };

    // Instantly notify frontend and raise main window! Zero lag!
    if let Some(main_win) = app.get_webview_window("main") {
        force_raise_window(&main_win);
        main_win
            .emit("capture-finished", &loaded_res)
            .map_err(|e| e.to_string())?;

        // Accessibility queries can block indefinitely in another application.
        // Deliver the image first, then add snap targets only if the query finishes.
        if let (Some(target), Some(history_id)) = (selected_window, loaded_res.history_id.clone()) {
            let _ = main_win.emit(
                "capture-ui-scan-status",
                serde_json::json!({
                    "history_id": history_id, "status": "started"
                }),
            );
            // A restarted app can reuse its window title. Cache timeouts by process
            // so a stale accessibility endpoint does not suppress its replacement.
            let target_key = match (
                target.window_id.as_deref(),
                crate::ui_elements::target_process_id(&target),
            ) {
                (Some(id), pid) => format!("id:{id}:pid:{pid:?}"),
                (None, Some(pid)) => format!("pid:{pid}"),
                (None, None) => format!(
                    "window:{}:{:.0}:{:.0}:{:.0}:{:.0}",
                    target.name.as_deref().unwrap_or_default(),
                    target.x,
                    target.y,
                    target.width,
                    target.height
                ),
            };
            let known_timeout = DETAIL_SCAN_FAILURES
                .get_or_init(Default::default)
                .lock()
                .is_ok_and(|keys| keys.contains(&target_key));
            if !known_timeout {
                let app_for_scan = app.clone();
                tauri::async_runtime::spawn(async move {
                    let wait_started = Instant::now();
                    while DETAIL_SCAN_RUNNING
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_err()
                    {
                        if app_for_scan
                            .state::<crate::AppState>()
                            .capture_generation
                            .load(Ordering::SeqCst)
                            != generation
                        {
                            return;
                        }
                        if wait_started.elapsed() >= Duration::from_secs(4) {
                            if let Some(main) = app_for_scan.get_webview_window("main") {
                                let _ = main.emit(
                                    "capture-ui-scan-status",
                                    serde_json::json!({
                                        "history_id": history_id, "status": "timeout"
                                    }),
                                );
                            }
                            return;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    let timed_out_while_waiting = DETAIL_SCAN_FAILURES
                        .get_or_init(Default::default)
                        .lock()
                        .is_ok_and(|keys| keys.contains(&target_key));
                    if timed_out_while_waiting
                        || app_for_scan
                            .state::<crate::AppState>()
                            .capture_generation
                            .load(Ordering::SeqCst)
                            != generation
                    {
                        DETAIL_SCAN_RUNNING.store(false, Ordering::SeqCst);
                        if timed_out_while_waiting {
                            if let Some(main) = app_for_scan.get_webview_window("main") {
                                let _ = main.emit(
                                    "capture-ui-scan-status",
                                    serde_json::json!({
                                        "history_id": history_id, "status": "timeout"
                                    }),
                                );
                            }
                        }
                        return;
                    }
                    let scan = tauri::async_runtime::spawn_blocking(move || {
                        // Keep the slot occupied until the blocking call actually exits,
                        // including after the async timeout or a panic.
                        let _guard = DetailScanGuard;
                        crate::ui_elements::capture_desktop_detailed_elements_for_window(
                            0, 0, &target,
                        )
                    });
                    let scan_result = tokio::time::timeout(Duration::from_secs(3), scan).await;
                    if let Ok(Ok(elements)) = scan_result {
                        let elements = if let Some((x, y, w, h)) = scan_crop {
                            crate::ui_elements::filter_elements_for_crop(&elements, x, y, w, h)
                        } else {
                            elements
                        };
                        if app_for_scan
                            .state::<crate::AppState>()
                            .capture_generation
                            .load(Ordering::SeqCst)
                            == generation
                        {
                            if let Some(main) = app_for_scan.get_webview_window("main") {
                                let _ = main.emit(
                                    "capture-ui-elements",
                                    serde_json::json!({
                                        "history_id": history_id, "elements": elements
                                    }),
                                );
                            }
                        }
                    } else {
                        if let Ok(mut keys) =
                            DETAIL_SCAN_FAILURES.get_or_init(Default::default).lock()
                        {
                            keys.insert(target_key);
                        }
                        if app_for_scan
                            .state::<crate::AppState>()
                            .capture_generation
                            .load(Ordering::SeqCst)
                            == generation
                        {
                            if let Some(main) = app_for_scan.get_webview_window("main") {
                                let _ = main.emit(
                                    "capture-ui-scan-status",
                                    serde_json::json!({
                                        "history_id": history_id, "status": "timeout"
                                    }),
                                );
                            }
                        }
                    }
                });
            } else {
                let _ = main_win.emit(
                    "capture-ui-scan-status",
                    serde_json::json!({
                        "history_id": history_id, "status": "timeout"
                    }),
                );
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub fn cmd_cancel_capture(app: tauri::AppHandle) -> Result<(), String> {
    app.state::<crate::AppState>()
        .capture_generation
        .fetch_add(1, Ordering::SeqCst);
    if let Ok(mut pending) = app.state::<crate::AppState>().pending_capture.lock() {
        pending.take();
    }
    if let Some(overlay_win) = app.get_webview_window("overlay") {
        let _ = overlay_win.hide();
    }
    if let Some(main_win) = app.get_webview_window("main") {
        force_raise_window(&main_win);
    }
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct CropRectInput {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[tauri::command]
pub async fn cmd_fetch_detailed_ui_elements(
    crop_rect: Option<CropRectInput>,
) -> Result<Vec<crate::ui_elements::DetectedUiElement>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let filter = crop_rect.map(|c| (c.x, c.y, c.width, c.height));
        Ok(crate::ui_elements::capture_desktop_detailed_elements(
            0, 0, filter,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn force_raise_window(window: &tauri::WebviewWindow) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();

    crate::ui_elements::activate_x11_window(Some("MarkIts Editor"));
}

pub fn show_overlay_window(window: &tauri::WebviewWindow) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();

    crate::ui_elements::activate_x11_window(Some("MarkIts Capture Overlay"));
}

#[tauri::command]
pub fn cmd_raise_window(window: tauri::WebviewWindow) -> Result<(), String> {
    force_raise_window(&window);
    Ok(())
}

#[tauri::command]
pub async fn cmd_crop_and_load(
    raw_data_url: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    ui_elements: Option<Vec<crate::ui_elements::DetectedUiElement>>,
) -> Result<metadata::LoadedImageResult, String> {
    let prefix = "data:image/png;base64,";
    let base64_str = if raw_data_url.starts_with(prefix) {
        &raw_data_url[prefix.len()..]
    } else if let Some(pos) = raw_data_url.find(',') {
        &raw_data_url[pos + 1..]
    } else {
        &raw_data_url
    };

    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_str)
        .map_err(|e| format!("Base64 decode error: {}", e))?;

    let dynamic_img = image::load_from_memory(&png_bytes).map_err(|e| e.to_string())?;
    let rgba = dynamic_img.to_rgba8();
    let cropped =
        capture::crop_rgba_image(&rgba, x, y, width, height).map_err(|e| e.to_string())?;
    let captured = capture::rgba_to_captured_image(&cropped).map_err(|e| e.to_string())?;

    let cropped_elements = ui_elements.map(|elements| {
        crate::ui_elements::filter_elements_for_crop(
            &elements,
            x as f64,
            y as f64,
            width as f64,
            height as f64,
        )
    });

    // Auto-save new capture into history with cropped UI elements
    let item = history::save_or_update_history_item(
        None,
        &captured.raw_png,
        None,
        cropped_elements.as_deref(),
        None,
        None,
    )
    .map_err(|e| e.to_string())?;

    Ok(metadata::LoadedImageResult {
        width: captured.width,
        height: captured.height,
        image_data_url: captured.data_url,
        annotations_json: None,
        history_id: Some(item.id),
        ui_elements: cropped_elements,
        base_image_data_url: None,
        base_width: None,
        base_height: None,
        base_ui_elements: None,
        crop_info: None,
    })
}

fn decode_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let prefix = "data:";
    let base64_str = if data_url.starts_with(prefix) {
        if let Some(pos) = data_url.find(',') {
            &data_url[pos + 1..]
        } else {
            data_url
        }
    } else {
        data_url
    };
    base64::engine::general_purpose::STANDARD
        .decode(base64_str)
        .map_err(|e| format!("Base64 decode error: {}", e))
}

#[tauri::command]
pub async fn cmd_save_to_history(
    background_data_url: String,
    scene_json: Option<String>,
    existing_id: Option<String>,
    ui_elements: Option<Vec<crate::ui_elements::DetectedUiElement>>,
    base_background_data_url: Option<String>,
    crop_info: Option<metadata::CropInfo>,
) -> Result<history::HistoryItem, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bg_bytes = decode_data_url(&background_data_url)?;
        let base_bg_bytes = if let Some(ref base_url) = base_background_data_url {
            if base_url != &background_data_url {
                Some(decode_data_url(base_url)?)
            } else {
                None
            }
        } else {
            None
        };

        history::save_or_update_history_item(
            existing_id.as_deref(),
            &bg_bytes,
            scene_json.as_deref(),
            ui_elements.as_deref(),
            base_bg_bytes.as_deref(),
            crop_info.as_ref(),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn cmd_load_image(file_path: String) -> Result<metadata::LoadedImageResult, String> {
    let bytes = fs::read(&file_path).map_err(|e| e.to_string())?;
    metadata::load_image_with_metadata(&bytes).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_get_history() -> Result<Vec<HistoryItem>, String> {
    tauri::async_runtime::spawn_blocking(|| history::list_history().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn cmd_load_history_item(id: String) -> Result<metadata::LoadedImageResult, String> {
    history::load_history_item(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_delete_history_item(id: String) -> Result<(), String> {
    history::delete_history_item(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_render_svg(scene_json: String) -> Result<String, String> {
    markits::render_from_json(&scene_json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_compose_and_save(
    background_data_url: String,
    scene_json: String,
    save_path: String,
    ui_elements: Option<Vec<crate::ui_elements::DetectedUiElement>>,
    export_width: Option<u32>,
    export_height: Option<u32>,
    reproduction_json: bool,
) -> Result<(), String> {
    let prefix = "data:";
    let base64_str = if background_data_url.starts_with(prefix) {
        if let Some(pos) = background_data_url.find(',') {
            &background_data_url[pos + 1..]
        } else {
            &background_data_url
        }
    } else {
        &background_data_url
    };

    let bg_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_str)
        .map_err(|e| format!("Base64 decode error: {}", e))?;

    // Render composed PNG using MarkIts core raster renderer
    let composed_png =
        markits::render_composed_png_bytes(&scene_json, &bg_bytes).map_err(|e| e.to_string())?;

    let final_png_bytes = if let (Some(ew), Some(eh)) = (export_width, export_height) {
        if ew > 0 && eh > 0 {
            let dynamic_img = image::load_from_memory(&composed_png).map_err(|e| e.to_string())?;
            if dynamic_img.width() != ew || dynamic_img.height() != eh {
                let resized =
                    dynamic_img.resize_exact(ew, eh, image::imageops::FilterType::Lanczos3);
                let mut cursor = std::io::Cursor::new(Vec::new());
                resized
                    .write_to(&mut cursor, image::ImageFormat::Png)
                    .map_err(|e| e.to_string())?;
                cursor.into_inner()
            } else {
                composed_png
            }
        } else {
            composed_png
        }
    } else {
        composed_png
    };

    // Embed annotations metadata and UI elements into PNG tEXt chunks for re-editing
    let final_png = metadata::embed_metadata(
        &final_png_bytes,
        Some(&scene_json),
        ui_elements.as_deref(),
        None,
    )
    .map_err(|e| e.to_string())?;
    let final_png = metadata::embed_text_chunk(
        &final_png,
        metadata::MARKITS_SOURCE_KEYWORD,
        &background_data_url,
    )
    .map_err(|e| e.to_string())?;

    fs::write(&save_path, &final_png).map_err(|e| e.to_string())?;

    if reproduction_json {
        let spec = create_reproduction_spec(&scene_json, ui_elements.as_deref())?;
        let spec_path = std::path::Path::new(&save_path).with_extension("json");
        let spec_bytes = serde_json::to_vec_pretty(&spec).map_err(|e| e.to_string())?;
        fs::write(spec_path, spec_bytes).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn create_reproduction_spec(
    scene_json: &str,
    ui_elements: Option<&[crate::ui_elements::DetectedUiElement]>,
) -> Result<serde_json::Value, String> {
    let mut spec: serde_json::Value =
        serde_json::from_str(scene_json).map_err(|e| e.to_string())?;
    let Some(elements) = ui_elements else {
        return Ok(spec);
    };
    let Some(annotations) = spec
        .get_mut("annotations")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(spec);
    };

    for annotation in annotations {
        let Some(target) = annotation.get_mut("target") else {
            continue;
        };
        let values = if let Some(array) = target.as_array() {
            if array.len() != 4 {
                continue;
            }
            let Some(numbers) = array
                .iter()
                .map(serde_json::Value::as_f64)
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            numbers
        } else if target.is_object() {
            let fields = ["x", "y", "width", "height"];
            let Some(numbers) = fields
                .iter()
                .map(|field| target.get(*field).and_then(serde_json::Value::as_f64))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            numbers
        } else {
            continue;
        };
        let matches: Vec<_> = elements
            .iter()
            .filter(|element| {
                element
                    .name
                    .as_deref()
                    .is_some_and(|name| !name.trim().is_empty())
                    && [element.x, element.y, element.width, element.height]
                        .iter()
                        .zip(&values)
                        .all(|(actual, expected)| actual.round() == expected.round())
            })
            .collect();
        if let [element] = matches.as_slice() {
            let name = element.name.as_deref().unwrap().trim();
            let unique_role_name = elements
                .iter()
                .filter(|candidate| {
                    candidate.role == element.role
                        && candidate
                            .name
                            .as_deref()
                            .is_some_and(|other| other.trim() == name)
                })
                .count()
                == 1;
            if unique_role_name {
                *target = serde_json::Value::String(format!("{}:{}", element.role, name));
            }
        }
    }
    Ok(spec)
}

#[tauri::command]
pub fn cmd_copy_to_clipboard(
    background_data_url: String,
    scene_json: String,
    export_width: Option<u32>,
    export_height: Option<u32>,
) -> Result<(), String> {
    let prefix = "data:";
    let base64_str = if background_data_url.starts_with(prefix) {
        if let Some(pos) = background_data_url.find(',') {
            &background_data_url[pos + 1..]
        } else {
            &background_data_url
        }
    } else {
        &background_data_url
    };

    let bg_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_str)
        .map_err(|e| format!("Base64 decode error: {}", e))?;

    let composed_png =
        markits::render_composed_png_bytes(&scene_json, &bg_bytes).map_err(|e| e.to_string())?;

    let img = image::load_from_memory(&composed_png).map_err(|e| e.to_string())?;
    let img = if let (Some(ew), Some(eh)) = (export_width, export_height) {
        if ew > 0 && eh > 0 && (ew != img.width() || eh != img.height()) {
            img.resize_exact(ew, eh, image::imageops::FilterType::Lanczos3)
        } else {
            img
        }
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (width, height) = img.dimensions();

    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard
        .set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: std::borrow::Cow::Borrowed(&rgba),
        })
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::io::Cursor;

    fn create_test_bg_data_url(width: u32, height: u32) -> String {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(width, height, Rgba([200, 200, 200, 255]));
        let mut buffer = Cursor::new(Vec::new());
        img.write_to(&mut buffer, image::ImageFormat::Png).unwrap();
        let bytes = buffer.into_inner();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        format!("data:image/png;base64,{}", b64)
    }

    #[test]
    fn test_cmd_render_svg() {
        let scene_json = r#"{"canvas":{"width":400,"height":300},"annotations":[{"type":"rect","target":[10,10,100,50]}]}"#;
        let svg = cmd_render_svg(scene_json.to_string()).unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("viewBox=\"0 0 400 300\""));
    }

    #[test]
    fn reproduction_spec_uses_unique_ui_names_for_snapped_targets() {
        let scene = r#"{"canvas":{"width":200,"height":150},"annotations":[{"type":"rect","target":[20,20,80,40]},{"type":"circle","target":[90,90,20,20]}]}"#;
        let elements = vec![crate::ui_elements::DetectedUiElement {
            role: "button".into(),
            name: Some("Save".into()),
            window_id: None,
            pid: None,
            x: 20.0,
            y: 20.0,
            width: 80.0,
            height: 40.0,
        }];
        let spec = create_reproduction_spec(scene, Some(&elements)).unwrap();
        assert_eq!(spec["annotations"][0]["target"], "button:Save");
        assert_eq!(
            spec["annotations"][1]["target"],
            serde_json::json!([90, 90, 20, 20])
        );
        assert!(
            spec.get("uimap").is_none(),
            "the sidecar should resolve names against the current image UIMap"
        );
    }

    #[test]
    fn test_cmd_compose_and_save_with_metadata_cycle() {
        let bg_url = create_test_bg_data_url(200, 150);
        let scene_json = r#"{"canvas":{"width":200,"height":150},"annotations":[{"type":"callout","target":[20,20,80,40],"text":"Test Note","style":"primary"}]}"#;

        let temp_dir = std::env::temp_dir();
        let test_output_path = temp_dir.join(format!("markits_test_{}.png", std::process::id()));
        let test_output_str = test_output_path.to_str().unwrap().to_string();

        let elements = vec![crate::ui_elements::DetectedUiElement {
            role: "button".into(),
            name: Some("OK".into()),
            window_id: None,
            pid: None,
            x: 20.0,
            y: 20.0,
            width: 80.0,
            height: 40.0,
        }];

        let save_res = cmd_compose_and_save(
            bg_url,
            scene_json.to_string(),
            test_output_str.clone(),
            Some(elements.clone()),
            None,
            None,
            true,
        );
        assert!(save_res.is_ok(), "compose_and_save failed: {:?}", save_res);

        // Verify that load_image restores image dimensions, annotations, and UI elements
        let loaded = cmd_load_image(test_output_str.clone()).unwrap();
        assert_eq!(loaded.width, 200);
        assert_eq!(loaded.height, 150);
        assert_eq!(loaded.image_data_url, create_test_bg_data_url(200, 150));
        assert!(loaded.annotations_json.is_some());
        assert!(loaded.ui_elements.is_some());
        let loaded_uis = loaded.ui_elements.unwrap();
        assert_eq!(loaded_uis.len(), 1);
        assert_eq!(loaded_uis[0].role, "button");
        assert_eq!(loaded_uis[0].name.as_deref(), Some("OK"));

        let restored_json = loaded.annotations_json.unwrap();
        assert!(restored_json.contains("Test Note"));
        assert!(restored_json.contains("callout"));

        let reproduction_path = std::path::Path::new(&test_output_str).with_extension("json");
        let reproduction: serde_json::Value =
            serde_json::from_slice(&fs::read(&reproduction_path).unwrap()).unwrap();
        assert_eq!(reproduction["annotations"][0]["target"], "button:OK");

        // Clean up
        let _ = fs::remove_file(test_output_path);
        let _ = fs::remove_file(reproduction_path);
    }

    #[test]
    fn test_cmd_compose_and_save_with_resolution_scaling() {
        let bg_url = create_test_bg_data_url(400, 300);
        let scene_json = r#"{"canvas":{"width":400,"height":300},"annotations":[{"type":"rect","target":[10,10,100,50]}]}"#;

        let temp_dir = std::env::temp_dir();
        let test_output_path =
            temp_dir.join(format!("markits_scale_test_{}.png", std::process::id()));
        let test_output_str = test_output_path.to_str().unwrap().to_string();

        let save_res = cmd_compose_and_save(
            bg_url,
            scene_json.to_string(),
            test_output_str.clone(),
            None,
            Some(800),
            Some(600),
            false,
        );
        assert!(save_res.is_ok(), "scaled save failed: {:?}", save_res);

        let exported = image::open(&test_output_path).unwrap();
        assert_eq!((exported.width(), exported.height()), (800, 600));
        let loaded = cmd_load_image(test_output_str).unwrap();
        // Editing uses the original scene coordinate system.
        assert_eq!((loaded.width, loaded.height), (400, 300));
        assert_eq!(loaded.image_data_url, create_test_bg_data_url(400, 300));

        let _ = fs::remove_file(test_output_path);
    }
}
