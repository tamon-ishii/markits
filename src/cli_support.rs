//! Image export and desktop discovery used by the CLI.
use crate::{metadata, raster};
use base64::Engine;
use std::{error::Error, path::Path};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub fn write_image(path: &Path, png: &[u8]) -> Result<()> {
    let format = crate::image_export::format_for_path(&path.to_string_lossy())?;
    std::fs::write(path, crate::image_export::encode(png, &format)?)?;
    Ok(())
}

/// Render once, with an optional self-contained editable payload. Cropping uses
/// the exact bounds calculation used by the raster renderer.
pub fn render(json: &str, source: &[u8], margin: Option<u32>, editable: bool) -> Result<Vec<u8>> {
    let mut png = raster::render_composed_png_bytes_with_crop(json, source, margin)?;
    if !editable {
        return Ok(png);
    }
    let info = raster::inspect_image_bytes(source)?;
    let prepared =
        raster::with_image_canvas_and_uimap(json, info.width, info.height, info.uimap.as_deref())?;
    crate::Scene::from_json(&prepared)?;
    let mut value: serde_json::Value = serde_json::from_str(&prepared)?;
    let crop = if let Some(margin) = margin {
        let layout = crate::render_with_layout_from_json(&prepared)?;
        raster::compute_annotation_bounds(&layout.elements).map(|[x, y, _, _]| {
            let x = (x - margin as f64).floor().max(0.0);
            let y = (y - margin as f64).floor().max(0.0);
            (x, y)
        })
    } else {
        None
    };
    let base = to_png(source)?;
    let mut background = base.clone();
    if let Some((x, y)) = crop {
        let out = image::load_from_memory(&png)?;
        background = crop_png(&base, x as u32, y as u32, out.width(), out.height())?;
        // Use semantic preparation's coordinate representation for all shapes.
        value = translate_scene(&prepared, x, y)?;
        value["canvas"] = serde_json::json!({"width":out.width(),"height":out.height()});
        let cropped_info = raster::inspect_image_bytes(&png)?;
        value["uimap"] = serde_json::to_value(cropped_info.uimap.unwrap_or_default())?;
        png = metadata::embed_crop_info(
            &png,
            &metadata::CropInfo {
                is_auto_cropped: true,
                offset_x: x,
                offset_y: y,
                base_width: info.width,
                base_height: info.height,
            },
        )?;
        png = metadata::embed_text_chunk(&png, "markits:base_image", &data_url(&base))?;
    }
    png = metadata::embed_annotations(&png, &serde_json::to_string(&value)?)?;
    png = metadata::embed_text_chunk(
        &png,
        metadata::MARKITS_SOURCE_KEYWORD,
        &data_url(&background),
    )?;
    Ok(crate::recording_metadata::copy_metadata(&png, source)?)
}

pub fn translate_scene(json: &str, dx: f64, dy: f64) -> Result<serde_json::Value> {
    let prepared = crate::semantic::prepare(json)?;
    let mut value = serde_json::to_value(prepared.scene)?;
    // Model serialization uses rectangle objects and explicit point arrays.
    fn shift(value: &mut serde_json::Value, dx: f64, dy: f64) {
        if let Some(obj) = value.as_object_mut() {
            if let (Some(x), Some(y)) = (
                obj.get("x").and_then(|v| v.as_f64()),
                obj.get("y").and_then(|v| v.as_f64()),
            ) {
                obj.insert("x".into(), serde_json::json!(x - dx));
                obj.insert("y".into(), serde_json::json!(y - dy));
            }
            for key in [
                "target", "from", "to", "control", "control1", "control2", "start", "end",
            ] {
                if let Some(child) = obj.get_mut(key) {
                    if let Some(a) = child.as_array_mut() {
                        if a.len() >= 2 {
                            if let (Some(x), Some(y)) = (a[0].as_f64(), a[1].as_f64()) {
                                a[0] = serde_json::json!(x - dx);
                                a[1] = serde_json::json!(y - dy);
                            }
                        }
                    } else {
                        shift(child, dx, dy);
                    }
                }
            }
        }
    }
    if let Some(items) = value["annotations"].as_array_mut() {
        for item in items {
            shift(item, dx, dy);
        }
    }
    Ok(value)
}

pub fn data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}
pub fn to_png(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(bytes.to_vec());
    }
    let normalized = crate::image_export::to_editable_png(bytes)?;
    if normalized.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(normalized);
    }
    let image = image::load_from_memory(&normalized)?;
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}
pub fn crop_png(bytes: &[u8], x: u32, y: u32, w: u32, h: u32) -> Result<Vec<u8>> {
    let image = image::load_from_memory(bytes)?;
    let cropped = raster::crop_image(&image, x, y, w, h)?;
    let mut out = std::io::Cursor::new(Vec::new());
    cropped.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}

pub fn native_window(
    query: Option<&str>,
    pid: Option<u32>,
) -> Result<crate::window_capture::WindowInfo> {
    let windows = crate::window_capture::list_windows()?;
    let matching: Vec<_> = windows
        .into_iter()
        .filter(|w| {
            if let Some(pid) = pid {
                crate::semantic_target::window_context(&w.id).is_ok_and(|c| c.pid == Some(pid))
            } else if let Some(q) = query {
                w.id == q || w.title.to_lowercase().contains(&q.to_lowercase())
            } else {
                false
            }
        })
        .collect();
    if matching.len() != 1 {
        return Err(format!(
            "Expected one target window, found {}. Use a unique window ID or PID.",
            matching.len()
        )
        .into());
    }
    Ok(matching.into_iter().next().unwrap())
}

pub fn cursor(png: &[u8], bounds: &crate::window_capture::WindowInfo) -> Result<Vec<u8>> {
    #[cfg(target_os = "windows")]
    {
        let mut image = image::load_from_memory(png)?.to_rgba8();
        crate::cursor_capture::windows::draw_current_cursor(&mut image, bounds)?;
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png)?;
        Ok(out.into_inner())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (png, bounds);
        Err("--include-cursor is supported only on Windows".into())
    }
}
