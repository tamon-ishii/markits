use base64::Engine;
use image::{DynamicImage, ImageFormat};
use crate::{Scene, Theme, UiElement, renderer, render_from_json};
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

pub fn render_composed_png_bytes(json: &str, image_bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
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

    let svg = render_from_json(&resolved)?;
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
    let rendered_png = pixmap.encode_png()?;
    if let Some(ref elements) = scene.uimap {
        if !elements.is_empty() {
            return embed_png_uimap(&rendered_png, elements);
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
}
