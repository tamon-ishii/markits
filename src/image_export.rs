//! Lossless export with a versioned MarkIts payload in standard XMP containers.
use crate::metadata;
use base64::Engine;
use image::{DynamicImage, ImageFormat};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Cursor, path::Path};

const JXL_SIGNATURE: &[u8] = b"\0\0\0\x0cJXL \x0d\x0a\x87\x0a";
const PAYLOAD_START: &str = "<markits:payload>";
const PAYLOAD_END: &str = "</markits:payload>";
const KEYS: &[&str] = &[
    metadata::MARKITS_KEYWORD,
    metadata::MARKITS_UI_ELEMENTS_KEYWORD,
    metadata::MARKITS_CROP_KEYWORD,
    metadata::MARKITS_SOURCE_KEYWORD,
    metadata::MARKITS_CAPTURE_TARGET_KEYWORD,
    "markits:base_image",
    crate::recording_metadata::OPERATIONS_KEY,
    crate::recording_metadata::GROUP_KEY,
    crate::recording_metadata::RECIPE_KEY,
];

#[derive(Serialize, Deserialize)]
struct EditMetadata {
    version: u32,
    text: BTreeMap<String, String>,
}

pub fn format_for_path(path: &str) -> Result<String, String> {
    let extension = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" | "webp" | "jxl" => Ok(extension),
        _ => Err("Choose PNG, WebP, or JPEG XL for saving".into()),
    }
}

fn xmp_from_png(png: &[u8]) -> Result<Vec<u8>, String> {
    let mut text = BTreeMap::new();
    for &key in KEYS {
        if let Some(value) = metadata::extract_text_chunk(png, key).map_err(|e| e.to_string())? {
            text.insert(key.to_string(), value);
        }
    }
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let data = serde_json::to_vec(&EditMetadata { version: 1, text }).map_err(|e| e.to_string())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    Ok(format!(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:markits="https://github.com/tamon-ishii/markits/ns/1.0/">{PAYLOAD_START}{encoded}{PAYLOAD_END}</rdf:Description></rdf:RDF></x:xmpmeta>"#).into_bytes())
}

fn append_box(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Result<(), String> {
    let size = u32::try_from(data.len())
        .ok()
        .and_then(|n| n.checked_add(8))
        .ok_or("JPEG XL box is too large")?;
    output.extend_from_slice(&size.to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    Ok(())
}

pub fn encode(png: &[u8], format: &str) -> Result<Vec<u8>, String> {
    if format == "png" {
        return Ok(png.to_vec());
    }
    let rgba = image::load_from_memory(png)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let xmp = xmp_from_png(png)?;
    let mut output = Vec::new();
    match format {
        "webp" => {
            let mut encoder = image_webp::WebPEncoder::new(&mut output);
            encoder.set_xmp_metadata(xmp);
            encoder
                .encode(
                    &rgba,
                    rgba.width(),
                    rgba.height(),
                    image_webp::ColorType::Rgba8,
                )
                .map_err(|e| e.to_string())?;
        }
        "jxl" => {
            let options = zune_core::options::EncoderOptions::new(
                rgba.width() as usize,
                rgba.height() as usize,
                zune_core::colorspace::ColorSpace::RGBA,
                zune_core::bit_depth::BitDepth::Eight,
            );
            let mut codestream = Vec::new();
            zune_jpegxl::JxlSimpleEncoder::new(&rgba, options)
                .encode(&mut codestream)
                .map_err(|e| format!("JPEG XL: {e:?}"))?;
            output.extend_from_slice(JXL_SIGNATURE);
            append_box(&mut output, b"ftyp", b"jxl \0\0\0\0jxl ")?;
            if !xmp.is_empty() {
                append_box(&mut output, b"xml ", &xmp)?;
            }
            append_box(&mut output, b"jxlc", &codestream)?;
        }
        _ => return Err("Unsupported image export format".into()),
    }
    Ok(output)
}

pub(crate) fn is_webp(bytes: &[u8]) -> bool {
    bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")
}

pub(crate) fn is_jxl(bytes: &[u8]) -> bool {
    bytes.starts_with(JXL_SIGNATURE) || bytes.starts_with(b"\xff\x0a")
}

// Read only bounded container chunks; never search compressed pixels for tags.
fn xmp(bytes: &[u8]) -> Result<Option<&[u8]>, String> {
    let webp = is_webp(bytes);
    if !webp && !bytes.starts_with(JXL_SIGNATURE) {
        return Ok(None);
    }
    let mut offset: usize = if webp { 12 } else { 0 };
    let end = if webp {
        let size = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        size.checked_add(8)
            .filter(|&end| end <= bytes.len())
            .ok_or("Truncated WebP container")?
    } else {
        bytes.len()
    };
    while offset < end {
        let header = bytes
            .get(offset..offset.checked_add(8).ok_or("Invalid box offset")?)
            .filter(|_| offset + 8 <= end)
            .ok_or("Truncated metadata box")?;
        let (kind, size, header_len) = if webp {
            (
                &header[..4],
                u32::from_le_bytes(header[4..].try_into().unwrap()) as usize + 8,
                8,
            )
        } else {
            let size = u32::from_be_bytes(header[..4].try_into().unwrap());
            let (size, header_len) = match size {
                0 => (end - offset, 8),
                1 => {
                    let large = bytes
                        .get(offset + 8..offset + 16)
                        .filter(|_| offset + 16 <= end)
                        .ok_or("Truncated large box")?;
                    (
                        usize::try_from(u64::from_be_bytes(large.try_into().unwrap()))
                            .map_err(|_| "Box too large")?,
                        16,
                    )
                }
                n => (n as usize, 8),
            };
            (&header[4..], size, header_len)
        };
        let next = offset
            .checked_add(size)
            .filter(|&n| size >= header_len && n <= end)
            .ok_or("Invalid metadata box size")?;
        if kind == if webp { b"XMP " } else { b"xml " } {
            let data = &bytes[offset + header_len..next];
            if data
                .windows(PAYLOAD_START.len())
                .any(|s| s == PAYLOAD_START.as_bytes())
            {
                return Ok(Some(data));
            }
        }
        offset = next
            .checked_add(if webp { size % 2 } else { 0 })
            .ok_or("Invalid padding")?;
        if offset > end {
            return Err("Missing WebP chunk padding".into());
        }
    }
    Ok(None)
}

fn extract_metadata(bytes: &[u8]) -> Result<Option<EditMetadata>, String> {
    let Some(xmp) = xmp(bytes)? else {
        return Ok(None);
    };
    let xml = std::str::from_utf8(xmp).map_err(|e| e.to_string())?;
    let value = xml
        .split_once(PAYLOAD_START)
        .and_then(|(_, s)| s.split_once(PAYLOAD_END))
        .ok_or("Invalid MarkIts XMP payload")?
        .0;
    let json = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|e| e.to_string())?;
    let metadata: EditMetadata = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
    if metadata.version != 1 {
        return Err("Unsupported MarkIts XMP version".into());
    }
    Ok(Some(metadata))
}

fn decode(bytes: &[u8]) -> Result<DynamicImage, String> {
    if !is_jxl(bytes) {
        return image::load_from_memory(bytes).map_err(|e| e.to_string());
    }
    let image = jxl_oxide::JxlImage::builder()
        .read(Cursor::new(bytes))
        .map_err(|e| e.to_string())?;
    let render = image.render_frame(0).map_err(|e| e.to_string())?;
    let mut stream = render.stream();
    let (width, height, channels) = (stream.width(), stream.height(), stream.channels());
    let len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(channels as usize))
        .ok_or("Image is too large")?;
    let mut pixels = vec![0u8; len];
    if stream.write_to_buffer(&mut pixels) != len {
        return Err("Incomplete JPEG XL image".into());
    }
    let image = match channels {
        1 => DynamicImage::ImageLuma8(
            image::GrayImage::from_raw(width, height, pixels).ok_or("Invalid pixels")?,
        ),
        2 => DynamicImage::ImageLumaA8(
            image::GrayAlphaImage::from_raw(width, height, pixels).ok_or("Invalid pixels")?,
        ),
        3 => DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(width, height, pixels).ok_or("Invalid pixels")?,
        ),
        4 => DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(width, height, pixels).ok_or("Invalid pixels")?,
        ),
        _ => return Err("Unsupported JPEG XL channel count".into()),
    };
    Ok(image)
}

/// Normalize imported WebP/JXL to the editor's PNG representation, restoring
/// all edit metadata so existing crop, UIMap and playback code remains shared.
pub fn to_editable_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if !is_webp(bytes) && !is_jxl(bytes) {
        return Ok(bytes.to_vec());
    }
    let edits = extract_metadata(bytes)?;
    let image = decode(bytes)?;
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let mut png = output.into_inner();
    if let Some(edits) = edits {
        for (key, value) in edits.text {
            if KEYS.contains(&key.as_str()) {
                png = metadata::embed_text_chunk(&png, &key, &value).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_png() -> Vec<u8> {
        let rgba = image::RgbaImage::from_fn(31, 19, |x, y| {
            image::Rgba([
                (x * 7) as u8,
                (y * 11) as u8,
                ((x + y) * 3) as u8,
                if x < 3 { 80 } else { 255 },
            ])
        });
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(rgba)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        png.into_inner()
    }

    #[test]
    fn lossless_exports_decode_with_identical_pixels_and_transparency() {
        let png = sample_png();
        let expected = decode(&png).unwrap().into_rgba8();
        for format in ["png", "webp", "jxl"] {
            let encoded = encode(&png, format).unwrap();
            assert_eq!(decode(&encoded).unwrap().into_rgba8(), expected, "{format}");
            assert!(extract_metadata(&encoded).unwrap().is_none());
            assert_eq!(
                crate::image_metadata::strip_markits_metadata(&encoded).unwrap(),
                encoded
            );
        }
    }

    #[test]
    fn native_metadata_roundtrip_and_stripping_preserve_pixels() {
        let original = sample_png();
        let mut tagged = original.clone();
        for &key in KEYS {
            tagged = metadata::embed_text_chunk(&tagged, key, "元画像・UI・操作記録").unwrap();
        }
        for format in ["webp", "jxl"] {
            let encoded = encode(&tagged, format).unwrap();
            let restored = to_editable_png(&encoded).unwrap();
            for &key in KEYS {
                assert_eq!(
                    metadata::extract_text_chunk(&restored, key)
                        .unwrap()
                        .as_deref(),
                    Some("元画像・UI・操作記録")
                );
            }
            let stripped = crate::image_metadata::strip_markits_metadata(&encoded).unwrap();
            assert!(extract_metadata(&stripped).unwrap().is_none());
            assert_eq!(
                decode(&stripped).unwrap().into_rgba8(),
                decode(&original).unwrap().into_rgba8()
            );
            let restored = to_editable_png(&stripped).unwrap();
            for &key in KEYS {
                assert!(
                    metadata::extract_text_chunk(&restored, key)
                        .unwrap()
                        .is_none()
                );
            }
            assert_eq!(
                crate::image_metadata::strip_markits_metadata(&stripped).unwrap(),
                stripped
            );
        }
    }

    #[test]
    fn validates_format_extensions() {
        assert_eq!(format_for_path("capture.WEBP").unwrap(), "webp");
        assert_eq!(format_for_path("capture.jxl").unwrap(), "jxl");
        assert!(format_for_path("capture.gif").is_err());
    }
}
