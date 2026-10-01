use crate::capture;
use crate::history::{self, HistoryItem};
use crate::metadata;
use base64::Engine;
use image::GenericImageView;
use std::fs;
use tauri::Manager;

#[tauri::command]
pub async fn cmd_capture_screen(app: tauri::AppHandle) -> Result<capture::CapturedImage, String> {
    let window = app.get_webview_window("main");
    if let Some(ref w) = window {
        let _ = w.hide();
    }

    let res: Result<capture::CapturedImage, capture::CaptureError> = tauri::async_runtime::spawn_blocking(|| {
        // Allow window manager & compositor to hide MarkIts and redraw background
        std::thread::sleep(std::time::Duration::from_millis(250));
        let mut captured = capture::capture_primary_screen()?;
        let elements = crate::ui_elements::capture_desktop_ui_elements(0, 0);
        captured.ui_elements = elements;
        Ok(captured)
    })
    .await
    .map_err(|e| e.to_string())?;

    if let Some(ref w) = window {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }

    res.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_crop_and_load(
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
    let cropped = capture::crop_rgba_image(&rgba, x, y, width, height).map_err(|e| e.to_string())?;
    let captured = capture::rgba_to_captured_image(&cropped).map_err(|e| e.to_string())?;

    // Filter and offset UI elements for cropped region
    let cropped_elements = ui_elements.map(|els| {
        crate::ui_elements::filter_elements_for_crop(&els, x as f64, y as f64, width as f64, height as f64)
    });

    // Auto-save new capture into history with cropped UI elements
    let item = history::save_or_update_history_item(
        None,
        &captured.raw_png,
        None,
        cropped_elements.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    Ok(metadata::LoadedImageResult {
        width: captured.width,
        height: captured.height,
        image_data_url: captured.data_url,
        annotations_json: None,
        history_id: Some(item.id),
        ui_elements: cropped_elements,
    })
}

#[tauri::command]
pub async fn cmd_save_to_history(
    background_data_url: String,
    scene_json: Option<String>,
    existing_id: Option<String>,
    ui_elements: Option<Vec<crate::ui_elements::DetectedUiElement>>,
) -> Result<history::HistoryItem, String> {
    tauri::async_runtime::spawn_blocking(move || {
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

        history::save_or_update_history_item(
            existing_id.as_deref(),
            &bg_bytes,
            scene_json.as_deref(),
            ui_elements.as_deref(),
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
    tauri::async_runtime::spawn_blocking(|| {
        history::list_history().map_err(|e| e.to_string())
    })
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
                let resized = dynamic_img.resize_exact(ew, eh, image::imageops::FilterType::Lanczos3);
                let mut cursor = std::io::Cursor::new(Vec::new());
                resized.write_to(&mut cursor, image::ImageFormat::Png).map_err(|e| e.to_string())?;
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
    )
    .map_err(|e| e.to_string())?;

    fs::write(&save_path, &final_png).map_err(|e| e.to_string())?;

    Ok(())
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
    fn test_cmd_compose_and_save_with_metadata_cycle() {
        let bg_url = create_test_bg_data_url(200, 150);
        let scene_json = r#"{"canvas":{"width":200,"height":150},"annotations":[{"type":"callout","target":[20,20,80,40],"text":"Test Note","style":"primary"}]}"#;

        let temp_dir = std::env::temp_dir();
        let test_output_path = temp_dir.join(format!("markits_test_{}.png", std::process::id()));
        let test_output_str = test_output_path.to_str().unwrap().to_string();

        let elements = vec![crate::ui_elements::DetectedUiElement {
            role: "button".into(),
            name: Some("OK".into()),
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
        );
        assert!(save_res.is_ok(), "compose_and_save failed: {:?}", save_res);

        // Verify that load_image restores image dimensions, annotations, and UI elements
        let loaded = cmd_load_image(test_output_str).unwrap();
        assert_eq!(loaded.width, 200);
        assert_eq!(loaded.height, 150);
        assert!(loaded.annotations_json.is_some());
        assert!(loaded.ui_elements.is_some());
        let loaded_uis = loaded.ui_elements.unwrap();
        assert_eq!(loaded_uis.len(), 1);
        assert_eq!(loaded_uis[0].role, "button");
        assert_eq!(loaded_uis[0].name.as_deref(), Some("OK"));

        let restored_json = loaded.annotations_json.unwrap();
        assert!(restored_json.contains("Test Note"));
        assert!(restored_json.contains("callout"));

        // Clean up
        let _ = fs::remove_file(test_output_path);
    }

    #[test]
    fn test_cmd_compose_and_save_with_resolution_scaling() {
        let bg_url = create_test_bg_data_url(400, 300);
        let scene_json = r#"{"canvas":{"width":400,"height":300},"annotations":[{"type":"rect","target":[10,10,100,50]}]}"#;

        let temp_dir = std::env::temp_dir();
        let test_output_path = temp_dir.join(format!("markits_scale_test_{}.png", std::process::id()));
        let test_output_str = test_output_path.to_str().unwrap().to_string();

        let save_res = cmd_compose_and_save(
            bg_url,
            scene_json.to_string(),
            test_output_str.clone(),
            None,
            Some(800),
            Some(600),
        );
        assert!(save_res.is_ok(), "scaled save failed: {:?}", save_res);

        let loaded = cmd_load_image(test_output_str).unwrap();
        assert_eq!(loaded.width, 800);
        assert_eq!(loaded.height, 600);

        let _ = fs::remove_file(test_output_path);
    }
}
