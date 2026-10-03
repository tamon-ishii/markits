use base64::Engine;
use image::{DynamicImage, ImageFormat};
use crate::{Scene, Theme, UiElement, renderer};
use resvg::{tiny_skia, usvg};
use std::error::Error;
use std::fs;
use std::path::Path;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

pub fn extract_png_text_chunk(png_bytes: &[u8], target_keyword: &str) -> Option<String> {
    if png_bytes.len() < 8 || &png_bytes[0..8] != PNG_SIGNATURE {
        return None;
    }

    let mut cursor = std::io::Cursor::new(&png_bytes[8..]);
    use std::io::Read;

    while (cursor.position() as usize) < cursor.get_ref().len() {
        let mut len_buf = [0u8; 4];
        if cursor.read_exact(&mut len_buf).is_err() {
            break;
        }
        let length = u32::from_be_bytes(len_buf) as usize;

        let mut type_buf = [0u8; 4];
        if cursor.read_exact(&mut type_buf).is_err() {
            break;
        }

        if &type_buf == b"tEXt" {
            let mut data = vec![0u8; length];
            if cursor.read_exact(&mut data).is_err() {
                break;
            }
            let mut crc_buf = [0u8; 4];
            let _ = cursor.read_exact(&mut crc_buf);

            // Format: keyword + null separator (0x00) + text
            if let Some(null_pos) = data.iter().position(|&b| b == 0) {
                if let Ok(keyword) = std::str::from_utf8(&data[..null_pos]) {
                    if keyword == target_keyword {
                        if let Ok(text) = String::from_utf8(data[null_pos + 1..].to_vec()) {
                            return Some(text);
                        }
                    }
                }
            }
        } else if &type_buf == b"IEND" {
            break;
        } else {
            let skip = (length + 4) as u64;
            let new_pos = cursor.position() + skip;
            if new_pos > cursor.get_ref().len() as u64 {
                break;
            }
            cursor.set_position(new_pos);
        }
    }

    None
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

pub fn embed_png_text_chunk(
    png_bytes: &[u8],
    keyword: &str,
    text: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    if png_bytes.len() < 8 || &png_bytes[0..8] != PNG_SIGNATURE {
        return Err("Not a valid PNG image".into());
    }

    let mut cursor = std::io::Cursor::new(&png_bytes[8..]);
    use std::io::Read;

    let mut ihdr_len_buf = [0u8; 4];
    cursor.read_exact(&mut ihdr_len_buf)?;
    let ihdr_len = u32::from_be_bytes(ihdr_len_buf) as usize;

    let mut ihdr_type = [0u8; 4];
    cursor.read_exact(&mut ihdr_type)?;
    if &ihdr_type != b"IHDR" {
        return Err("PNG first chunk is not IHDR".into());
    }

    let mut ihdr_data = vec![0u8; ihdr_len];
    cursor.read_exact(&mut ihdr_data)?;
    let mut ihdr_crc = [0u8; 4];
    cursor.read_exact(&mut ihdr_crc)?;

    let mut text_data = Vec::with_capacity(keyword.len() + 1 + text.len());
    text_data.extend_from_slice(keyword.as_bytes());
    text_data.push(0);
    text_data.extend_from_slice(text.as_bytes());

    let mut to_crc = Vec::with_capacity(4 + text_data.len());
    to_crc.extend_from_slice(b"tEXt");
    to_crc.extend_from_slice(&text_data);
    let chunk_crc = crc32(&to_crc);

    let mut out = Vec::with_capacity(png_bytes.len() + 12 + text_data.len());
    out.extend_from_slice(PNG_SIGNATURE);

    out.extend_from_slice(&ihdr_len_buf);
    out.extend_from_slice(b"IHDR");
    out.extend_from_slice(&ihdr_data);
    out.extend_from_slice(&ihdr_crc);

    out.extend_from_slice(&(text_data.len() as u32).to_be_bytes());
    out.extend_from_slice(b"tEXt");
    out.extend_from_slice(&text_data);
    out.extend_from_slice(&chunk_crc.to_be_bytes());

    while (cursor.position() as usize) < cursor.get_ref().len() {
        let mut len_buf = [0u8; 4];
        if cursor.read_exact(&mut len_buf).is_err() {
            break;
        }
        let length = u32::from_be_bytes(len_buf) as usize;

        let mut type_buf = [0u8; 4];
        if cursor.read_exact(&mut type_buf).is_err() {
            break;
        }

        let mut data = vec![0u8; length];
        if cursor.read_exact(&mut data).is_err() {
            break;
        }

        let mut crc_buf = [0u8; 4];
        if cursor.read_exact(&mut crc_buf).is_err() {
            break;
        }

        if &type_buf == b"tEXt" {
            if let Some(null_pos) = data.iter().position(|&b| b == 0) {
                if let Ok(kw) = std::str::from_utf8(&data[..null_pos]) {
                    if kw == keyword {
                        continue;
                    }
                }
            }
        }

        out.extend_from_slice(&len_buf);
        out.extend_from_slice(&type_buf);
        out.extend_from_slice(&data);
        out.extend_from_slice(&crc_buf);

        if &type_buf == b"IEND" {
            break;
        }
    }

    Ok(out)
}

pub fn embed_png_uimap(
    png_bytes: &[u8],
    elements: &[UiElement],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let json = serde_json::to_string(elements)?;
    embed_png_text_chunk(png_bytes, "markits:ui_elements", &json)
}

pub fn extract_png_uimap(png_bytes: &[u8]) -> Option<Vec<UiElement>> {
    let json_str = extract_png_text_chunk(png_bytes, "markits:ui_elements")
        .or_else(|| extract_png_text_chunk(png_bytes, "markits:uimap"))?;
    serde_json::from_str(&json_str).ok()
}

pub fn load_uimap_from_path(path: &Path) -> Result<Vec<UiElement>, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    if bytes.starts_with(PNG_SIGNATURE) {
        if let Some(elements) = extract_png_uimap(&bytes) {
            return Ok(elements);
        }
        return Err(format!("No embedded UIMap found in PNG '{}'", path.display()).into());
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|e| format!("File '{}' is neither a valid PNG nor valid UTF-8 JSON: {e}", path.display()))?;
    let elements: Vec<UiElement> = serde_json::from_str(text)
        .map_err(|e| format!("Failed to parse UIMap JSON from '{}': {e}", path.display()))?;
    Ok(elements)
}

pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: &'static str,
    pub uimap: Option<Vec<UiElement>>,
}

fn read_image(path: &Path) -> Result<(Vec<u8>, ImageInfo), Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let (format, name) = match image::guess_format(&bytes)? {
        ImageFormat::Png => (ImageFormat::Png, "png"),
        ImageFormat::Jpeg => (ImageFormat::Jpeg, "jpeg"),
        _ => return Err("Image must contain a PNG or JPEG image".into()),
    };
    let uimap = if format == ImageFormat::Png {
        extract_png_uimap(&bytes)
    } else {
        None
    };
    let source = image::load_from_memory_with_format(&bytes, format)?;
    Ok((
        bytes,
        ImageInfo {
            width: source.width(),
            height: source.height(),
            format: name,
            uimap,
        },
    ))
}

pub fn inspect_image(path: &Path) -> Result<ImageInfo, Box<dyn Error>> {
    Ok(read_image(path)?.1)
}

fn crop_image(
    image: &DynamicImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<DynamicImage, Box<dyn Error>> {
    if width == 0
        || height == 0
        || x.checked_add(width)
            .is_none_or(|right| right > image.width())
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > image.height())
    {
        return Err(format!(
            "Crop rectangle ({x}, {y}, {width}, {height}) is outside image {}x{}",
            image.width(),
            image.height()
        )
        .into());
    }
    Ok(image.crop_imm(x, y, width, height))
}

pub fn crop_file(
    input: &Path,
    output: &Path,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn Error>> {
    if !output
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        return Err("--output must be a .png file".into());
    }
    let bytes = fs::read(input)?;
    let format = image::guess_format(&bytes)?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg) {
        return Err("Image must contain a PNG or JPEG image".into());
    }
    let image = image::load_from_memory_with_format(&bytes, format)?;
    let cropped = crop_image(&image, x, y, width, height)?;
    cropped.save_with_format(output, ImageFormat::Png)?;
    Ok(())
}

pub fn with_image_canvas(json: &str, width: u32, height: u32) -> Result<String, Box<dyn Error>> {
    with_image_canvas_and_uimap(json, width, height, None)
}

pub fn with_image_canvas_and_uimap(
    json: &str,
    width: u32,
    height: u32,
    uimap: Option<&[UiElement]>,
) -> Result<String, Box<dyn Error>> {
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    let root = value
        .as_object_mut()
        .ok_or("Annotation JSON root must be an object")?;
    if width > 0 && height > 0 {
        root.entry("canvas")
            .or_insert_with(|| serde_json::json!({"width": width, "height": height}));
    }

    if !root.contains_key("uimap") && !root.contains_key("ui_map") && !root.contains_key("ui_elements") {
        if let Some(elements) = uimap {
            if !elements.is_empty() {
                root.insert("uimap".to_string(), serde_json::to_value(elements)?);
            }
        }
    }
    Ok(serde_json::to_string(&value)?)
}

pub fn ensure_canvas_matches(scene: &Scene, info: &ImageInfo) -> Result<(), Box<dyn Error>> {
    if (info.width, info.height) != (scene.canvas.width, scene.canvas.height) {
        return Err(format!(
            "Image dimensions {}x{} do not match JSON canvas {}x{}",
            info.width, info.height, scene.canvas.width, scene.canvas.height
        )
        .into());
    }
    Ok(())
}

static SYSTEM_FONT_DATA: std::sync::LazyLock<(std::sync::Arc<usvg::fontdb::Database>, String)> =
    std::sync::LazyLock::new(|| {
        let mut fonts = usvg::fontdb::Database::new();
        fonts.load_system_fonts();
        let font_family = [
            "Arial",
            "DejaVu Sans",
            "Noto Sans",
            "Liberation Sans",
            "Helvetica",
        ]
        .into_iter()
        .find(|name| {
            fonts
                .faces()
                .any(|face| face.families.iter().any(|(family, _)| family == name))
        })
        .map(str::to_owned)
        .or_else(|| {
            fonts
                .faces()
                .next()
                .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
        })
        .unwrap_or_else(|| "sans-serif".to_string());

        (std::sync::Arc::new(fonts), font_family)
    });

pub fn compute_annotation_bounds(elements: &[renderer::LayoutElement]) -> Option<[f64; 4]> {
    if elements.is_empty() {
        return None;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for el in elements {
        let [x, y, w, h] = el.bounds;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);

        if let Some(ref path) = el.arrow_path {
            for &[px, py] in path {
                min_x = min_x.min(px);
                min_y = min_y.min(py);
                max_x = max_x.max(px);
                max_y = max_y.max(py);
            }
        }
    }

    if min_x > max_x || min_y > max_y || !min_x.is_finite() || !min_y.is_finite() {
        None
    } else {
        Some([min_x, min_y, max_x - min_x, max_y - min_y])
    }
}

pub fn filter_ui_elements_for_crop(
    elements: &[UiElement],
    crop_x: f64,
    crop_y: f64,
    crop_w: f64,
    crop_h: f64,
) -> Vec<UiElement> {
    elements
        .iter()
        .filter_map(|el| {
            if el.x.abs() < 1.0 && el.y.abs() < 1.0 && (el.role == "menuitem" || el.name.is_empty()) {
                return None;
            }
            let new_x = el.x - crop_x;
            let new_y = el.y - crop_y;
            if new_x < -4.0 || new_y < -4.0 || new_x >= crop_w || new_y >= crop_h {
                return None;
            }
            let inter_w = (new_x + el.width).min(crop_w) - new_x.max(0.0);
            let inter_h = (new_y + el.height).min(crop_h) - new_y.max(0.0);
            if inter_w < 6.0 || inter_h < 6.0 {
                return None;
            }
            if inter_w >= crop_w - 4.0 && inter_h >= crop_h - 4.0 {
                return None;
            }
            Some(UiElement {
                role: el.role.clone(),
                name: el.name.clone(),
                x: new_x.max(0.0),
                y: new_y.max(0.0),
                width: inter_w,
                height: inter_h,
            })
        })
        .collect()
}

pub fn render_composed_png_bytes(json: &str, image_bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    render_composed_png_bytes_with_crop(json, image_bytes, None)
}

pub fn render_composed_png_bytes_with_crop(
    json: &str,
    image_bytes: &[u8],
    crop_margin: Option<u32>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let format = image::guess_format(image_bytes)?;
    let format_str = match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        _ => return Err("Image must contain a PNG or JPEG image".into()),
    };
    let source = image::load_from_memory_with_format(image_bytes, format)?;
    let width = source.width();
    let height = source.height();

    let uimap = if format == ImageFormat::Png {
        extract_png_uimap(image_bytes)
    } else {
        None
    };
    let resolved = with_image_canvas_and_uimap(json, width, height, uimap.as_deref())?;
    let scene = Scene::from_json(&resolved)?;
    let info = ImageInfo {
        width,
        height,
        format: format_str,
        uimap,
    };
    ensure_canvas_matches(&scene, &info)?;

    let render_res = renderer::render_with_layout_from_json(&resolved)?;
    let svg = &render_res.svg;
    let root_end = svg.find('>').ok_or("Generated SVG has no root element")? + 1;
    let encoded = base64::engine::general_purpose::STANDARD.encode(image_bytes);
    let mut composed = String::with_capacity(svg.len() + encoded.len() + 200);
    composed.push_str(&svg[..root_end]);
    composed.push_str(&format!(
        "\n  <image x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" href=\"data:image/{};base64,{}\"/>",
        width, height, format_str, encoded
    ));
    composed.push_str(&svg[root_end..]);

    let (ref shared_db, ref default_font_family) = *SYSTEM_FONT_DATA;
    let mut options = usvg::Options::default();
    options.fontdb = std::sync::Arc::clone(shared_db);
    options.font_family = default_font_family.clone();
    let rendered_font = format!(
        "font-family=\"{}\"",
        renderer::escape_xml(&options.font_family)
    );
    let composed = composed.replace(
        &format!("font-family=\"{}\"", Theme::default().font_family),
        &rendered_font,
    );
    let tree = usvg::Tree::from_str(&composed, &options)?;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or("Image dimensions are too large to render")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let mut rendered_png = pixmap.encode_png()?;

    let mut crop_rect = None;
    if let Some(margin) = crop_margin {
        if let Some([bx, by, bw, bh]) = compute_annotation_bounds(&render_res.elements) {
            let m = margin as f64;
            let crop_x = (bx - m).floor().max(0.0) as u32;
            let crop_y = (by - m).floor().max(0.0) as u32;
            let crop_right = (bx + bw + m).ceil().min(width as f64) as u32;
            let crop_bottom = (by + bh + m).ceil().min(height as f64) as u32;
            let crop_w = crop_right.saturating_sub(crop_x).max(1);
            let crop_h = crop_bottom.saturating_sub(crop_y).max(1);

            let rendered_image = image::load_from_memory_with_format(&rendered_png, ImageFormat::Png)?;
            let cropped = crop_image(&rendered_image, crop_x, crop_y, crop_w, crop_h)?;
            let mut out = std::io::Cursor::new(Vec::new());
            cropped.write_to(&mut out, ImageFormat::Png)?;
            rendered_png = out.into_inner();
            crop_rect = Some((crop_x, crop_y, crop_w, crop_h));
        }
    }

    if let Some(ref elements) = scene.uimap {
        if !elements.is_empty() {
            let final_elements = if let Some((cx, cy, cw, ch)) = crop_rect {
                filter_ui_elements_for_crop(elements, cx as f64, cy as f64, cw as f64, ch as f64)
            } else {
                elements.clone()
            };
            if !final_elements.is_empty() {
                return embed_png_uimap(&rendered_png, &final_elements);
            }
        }
    }
    Ok(rendered_png)
}

pub fn render_png(json: &str, image_path: &Path, output_path: &Path) -> Result<(), Box<dyn Error>> {
    if !output_path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        return Err("--output must be a .png file".into());
    }

    let (image_bytes, _info) = read_image(image_path)?;
    let png_bytes = render_composed_png_bytes(json, &image_bytes)?;
    fs::write(output_path, png_bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgba, RgbaImage};

    #[test]
    fn crop_changes_dimensions_and_keeps_only_selected_pixels() {
        let source = DynamicImage::ImageRgba8(RgbaImage::from_fn(4, 3, |x, y| {
            Rgba([x as u8, y as u8, 99, 255])
        }));
        let cropped = crop_image(&source, 1, 1, 2, 2).unwrap();
        assert_eq!(cropped.dimensions(), (2, 2));
        assert_eq!(cropped.get_pixel(0, 0), Rgba([1, 1, 99, 255]));
        assert_eq!(cropped.get_pixel(1, 1), Rgba([2, 2, 99, 255]));
    }

    #[test]
    fn crop_rejects_empty_or_outside_rectangles() {
        let source = DynamicImage::ImageRgba8(RgbaImage::new(4, 3));
        assert!(crop_image(&source, 0, 0, 0, 2).is_err());
        assert!(crop_image(&source, 3, 0, 2, 2).is_err());
        assert!(crop_image(&source, u32::MAX, 0, 2, 2).is_err());
    }

    #[test]
    fn embed_and_extract_png_uimap_roundtrip() {
        let dummy_pixmap = tiny_skia::Pixmap::new(10, 10).unwrap();
        let png_bytes = dummy_pixmap.encode_png().unwrap();
        assert!(extract_png_uimap(&png_bytes).is_none());

        let elements = vec![
            UiElement {
                role: "button".to_string(),
                name: "保存".to_string(),
                x: 10.0,
                y: 20.0,
                width: 50.0,
                height: 30.0,
            },
            UiElement {
                role: "entry".to_string(),
                name: "検索".to_string(),
                x: 100.0,
                y: 20.0,
                width: 200.0,
                height: 30.0,
            },
        ];

        let embedded = embed_png_uimap(&png_bytes, &elements).expect("embed failed");
        let extracted = extract_png_uimap(&embedded).expect("extract failed");
        assert_eq!(extracted.len(), 2);
        assert_eq!(extracted[0].name, "保存");
        assert_eq!(extracted[0].x, 10.0);
        assert_eq!(extracted[1].name, "検索");

        // Overwrite with new elements
        let new_elements = vec![UiElement {
            role: "button".to_string(),
            name: "キャンセル".to_string(),
            x: 60.0,
            y: 20.0,
            width: 50.0,
            height: 30.0,
        }];
        let overwritten = embed_png_uimap(&embedded, &new_elements).expect("overwrite failed");
        let extracted_new = extract_png_uimap(&overwritten).expect("extract new failed");
        assert_eq!(extracted_new.len(), 1);
        assert_eq!(extracted_new[0].name, "キャンセル");
    }

    #[test]
    fn test_compute_annotation_bounds() {
        use crate::renderer::LayoutElement;
        let elements = vec![
            LayoutElement {
                id: "el1".to_string(),
                bounds: [100.0, 50.0, 80.0, 30.0],
                arrow_path: None,
            },
            LayoutElement {
                id: "el2".to_string(),
                bounds: [120.0, 100.0, 40.0, 20.0],
                arrow_path: Some(vec![[150.0, 150.0]]),
            },
        ];
        let bounds = compute_annotation_bounds(&elements).expect("Should compute bounds");
        assert_eq!(bounds, [100.0, 50.0, 80.0, 100.0]); // min_x: 100, min_y: 50, max_x: 180, max_y: 150 -> w: 80, h: 100
    }

    #[test]
    fn test_render_composed_png_bytes_with_crop() {
        let base_img = image::RgbaImage::new(400, 300);
        let mut png_bytes = Vec::new();
        base_img.write_to(&mut std::io::Cursor::new(&mut png_bytes), ImageFormat::Png).unwrap();

        let json = r#"{
            "annotations": [
                {
                    "type": "rect",
                    "target": [100, 100, 80, 40]
                }
            ]
        }"#;

        let cropped_bytes = render_composed_png_bytes_with_crop(json, &png_bytes, Some(10)).unwrap();
        let cropped_img = image::load_from_memory(&cropped_bytes).unwrap();
        assert_eq!(cropped_img.dimensions(), (100, 60));
    }
}
