use crate::metadata::{self, LoadedImageResult};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HistoryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Metadata error: {0}")]
    Metadata(#[from] metadata::MetadataError),
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("Item not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub timestamp: u64,
    pub date_formatted: String,
    pub width: u32,
    pub height: u32,
    pub file_path: String,
    pub thumbnail_data_url: String,
    pub has_annotations: bool,
}

pub fn get_history_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("markits").join("history");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn generate_thumbnail_bytes(bytes: &[u8], max_size: u32) -> Result<Vec<u8>, image::ImageError> {
    let img = image::load_from_memory(bytes)?;
    let thumb = img.thumbnail(max_size, max_size);
    let mut cursor = Cursor::new(Vec::new());
    thumb.write_to(&mut cursor, image::ImageFormat::Png)?;
    Ok(cursor.into_inner())
}

pub fn generate_thumbnail_data_url(bytes: &[u8], max_size: u32) -> Result<String, image::ImageError> {
    let thumb_bytes = generate_thumbnail_bytes(bytes, max_size)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&thumb_bytes);
    Ok(format!("data:image/png;base64,{}", b64))
}

use std::sync::atomic::{AtomicU64, Ordering};

static HISTORY_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn save_capture_to_history(
    png_bytes: &[u8],
    annotations_json: Option<&str>,
    ui_elements: Option<&[crate::ui_elements::DetectedUiElement]>,
) -> Result<HistoryItem, HistoryError> {
    save_or_update_history_item(None, png_bytes, annotations_json, ui_elements)
}

pub fn save_or_update_history_item(
    existing_id: Option<&str>,
    png_bytes: &[u8],
    annotations_json: Option<&str>,
    ui_elements: Option<&[crate::ui_elements::DetectedUiElement]>,
) -> Result<HistoryItem, HistoryError> {
    let history_dir = get_history_dir();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let now_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = HISTORY_COUNTER.fetch_add(1, Ordering::Relaxed);

    let id = existing_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("capture_{}_{}_{}", now_millis, std::process::id(), counter));
    let file_path = history_dir.join(format!("{}.png", id));
    let thumb_path = history_dir.join(format!("{}.thumb.png", id));

    // Embed annotations and UI elements if present
    let final_bytes = metadata::embed_metadata(png_bytes, annotations_json, ui_elements)?;

    fs::write(&file_path, &final_bytes)?;

    let header_info = metadata::inspect_png_header(png_bytes)?;
    let width = header_info.width;
    let height = header_info.height;

    // Render composed image for thumbnail if annotations exist
    let thumb_source_bytes = if let Some(json_str) = annotations_json {
        markits::render_composed_png_bytes(json_str, png_bytes).unwrap_or_else(|_| png_bytes.to_vec())
    } else {
        png_bytes.to_vec()
    };
    let thumb_bytes = generate_thumbnail_bytes(&thumb_source_bytes, 280)?;
    let _ = fs::write(&thumb_path, &thumb_bytes);

    let b64 = base64::engine::general_purpose::STANDARD.encode(&thumb_bytes);
    let thumbnail_data_url = format!("data:image/png;base64,{}", b64);

    let date_formatted = format_timestamp(now);

    Ok(HistoryItem {
        id,
        timestamp: now,
        date_formatted,
        width,
        height,
        file_path: file_path.to_string_lossy().to_string(),
        thumbnail_data_url,
        has_annotations: annotations_json.is_some(),
    })
}

pub fn list_history() -> Result<Vec<HistoryItem>, HistoryError> {
    let history_dir = get_history_dir();
    if !history_dir.exists() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();

    for entry in fs::read_dir(&history_dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Skip non-PNG files and thumbnail files
        if !file_name.ends_with(".png") || file_name.ends_with(".thumb.png") {
            continue;
        }

        let file_stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(stem) => stem,
            None => continue,
        };

        let metadata_fs = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let modified = metadata_fs
            .modified()
            .unwrap_or(SystemTime::now())
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let header_info = match metadata::inspect_png_header(&bytes) {
            Ok(info) => info,
            Err(_) => continue,
        };

        // Check if cached thumbnail exists on disk
        let thumb_path = history_dir.join(format!("{}.thumb.png", file_stem));
        let thumb_bytes = if thumb_path.exists() {
            fs::read(&thumb_path).ok()
        } else {
            None
        };

        let thumb_bytes = match thumb_bytes {
            Some(tb) => tb,
            None => {
                let thumb_source_bytes = if let Some(ref json) = header_info.annotations_json {
                    markits::render_composed_png_bytes(json, &bytes).unwrap_or_else(|_| bytes.clone())
                } else {
                    bytes.clone()
                };

                match generate_thumbnail_bytes(&thumb_source_bytes, 280) {
                    Ok(tb) => {
                        let _ = fs::write(&thumb_path, &tb);
                        tb
                    }
                    Err(_) => Vec::new(),
                }
            }
        };

        let thumbnail_data_url = if !thumb_bytes.is_empty() {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&thumb_bytes);
            format!("data:image/png;base64,{}", b64)
        } else {
            String::new()
        };

        items.push(HistoryItem {
            id: file_stem.to_string(),
            timestamp: modified,
            date_formatted: format_timestamp(modified),
            width: header_info.width,
            height: header_info.height,
            file_path: path.to_string_lossy().to_string(),
            thumbnail_data_url,
            has_annotations: header_info.has_annotations,
        });
    }

    // Sort newest first
    items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(items)
}

pub fn load_history_item(id: &str) -> Result<LoadedImageResult, HistoryError> {
    let history_dir = get_history_dir();
    let file_path = history_dir.join(format!("{}.png", id));
    if !file_path.exists() {
        return Err(HistoryError::NotFound(id.to_string()));
    }
    let bytes = fs::read(file_path)?;
    let mut res = metadata::load_image_with_metadata(&bytes)?;
    res.history_id = Some(id.to_string());
    Ok(res)
}

pub fn delete_history_item(id: &str) -> Result<(), HistoryError> {
    let history_dir = get_history_dir();
    let file_path = history_dir.join(format!("{}.png", id));
    let thumb_path = history_dir.join(format!("{}.thumb.png", id));
    if file_path.exists() {
        fs::remove_file(file_path)?;
    }
    if thumb_path.exists() {
        let _ = fs::remove_file(thumb_path);
    }
    Ok(())
}

fn format_timestamp(secs: u64) -> String {
    // Simple human-readable format without external chrono dependency
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = (time_of_day / 3600) % 24;
    let minutes = (time_of_day % 3600) / 60;

    // Approximate year and date
    let year = 1970 + days_since_epoch / 365;
    let day_of_year = (days_since_epoch % 365) + 1;
    let month = (day_of_year / 30).clamp(1, 12);
    let day = (day_of_year % 30).max(1);

    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hours, minutes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn create_dummy_png() -> Vec<u8> {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(16, 16, Rgba([0, 150, 255, 255]));
        let mut buffer = Cursor::new(Vec::new());
        img.write_to(&mut buffer, image::ImageFormat::Png).unwrap();
        buffer.into_inner()
    }

    #[test]
    fn test_save_load_delete_history() {
        use crate::ui_elements::DetectedUiElement;

        let dummy = create_dummy_png();
        let json = r#"{"canvas":{"width":16,"height":16},"annotations":[{"type":"rect","target":[2,2,8,8]}]}"#;
        let elements = vec![
            DetectedUiElement {
                role: "button".into(),
                name: Some("OK".into()),
                window_id: None, pid: None,
                x: 2.0,
                y: 2.0,
                width: 8.0,
                height: 8.0,
            }
        ];

        let item = save_capture_to_history(&dummy, Some(json), Some(&elements)).unwrap();
        assert_eq!(item.width, 16);
        assert_eq!(item.height, 16);
        assert!(item.has_annotations);

        let loaded = load_history_item(&item.id).unwrap();
        assert_eq!(loaded.width, 16);
        assert_eq!(loaded.annotations_json, Some(json.to_string()));
        assert!(loaded.ui_elements.is_some());
        assert_eq!(loaded.ui_elements.unwrap().len(), 1);

        let list = list_history().unwrap();
        assert!(list.iter().any(|h| h.id == item.id));

        delete_history_item(&item.id).unwrap();
        assert!(load_history_item(&item.id).is_err());
    }
}
