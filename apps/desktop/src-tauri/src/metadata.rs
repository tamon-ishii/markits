use base64::Engine;
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};
use thiserror::Error;

pub const MARKITS_KEYWORD: &str = "markits:annotations";
pub const MARKITS_UI_ELEMENTS_KEYWORD: &str = "markits:ui_elements";
pub const MARKITS_CROP_KEYWORD: &str = "markits:crop_info";
pub const MARKITS_SOURCE_KEYWORD: &str = "markits:source_image";
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Error, Debug)]
pub enum MetadataError {
    #[error("Invalid PNG signature")]
    InvalidSignature,
    #[error("Image decode error: {0}")]
    ImageDecode(#[from] image::ImageError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("UTF-8 decoding error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CropInfo {
    pub is_auto_cropped: bool,
    pub offset_x: f64,
    pub offset_y: f64,
    pub base_width: u32,
    pub base_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedImageResult {
    pub width: u32,
    pub height: u32,
    pub image_data_url: String,
    pub annotations_json: Option<String>,
    #[serde(default)]
    pub history_id: Option<String>,
    #[serde(default)]
    pub ui_elements: Option<Vec<crate::ui_elements::DetectedUiElement>>,
    #[serde(default)]
    pub base_image_data_url: Option<String>,
    #[serde(default)]
    pub base_width: Option<u32>,
    #[serde(default)]
    pub base_height: Option<u32>,
    #[serde(default)]
    pub base_ui_elements: Option<Vec<crate::ui_elements::DetectedUiElement>>,
    #[serde(default)]
    pub crop_info: Option<CropInfo>,
}

#[derive(Debug, Clone)]
pub struct PngHeaderInfo {
    pub width: u32,
    pub height: u32,
    pub has_annotations: bool,
    pub annotations_json: Option<String>,
    pub crop_info: Option<CropInfo>,
}

/// Inspect PNG header and metadata chunks without decoding pixel data.
pub fn inspect_png_header(png_bytes: &[u8]) -> Result<PngHeaderInfo, MetadataError> {
    if png_bytes.len() < 24 || &png_bytes[0..8] != PNG_SIGNATURE {
        return Err(MetadataError::InvalidSignature);
    }

    if &png_bytes[12..16] != b"IHDR" {
        return Err(MetadataError::InvalidSignature);
    }

    let width = u32::from_be_bytes(png_bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png_bytes[20..24].try_into().unwrap());

    let annotations_json = extract_annotations(png_bytes).unwrap_or(None);
    let has_annotations = annotations_json.is_some();
    let crop_info = extract_crop_info(png_bytes).unwrap_or(None);

    Ok(PngHeaderInfo {
        width,
        height,
        has_annotations,
        annotations_json,
        crop_info,
    })
}

/// Extract arbitrary tEXt chunk value for the given keyword.
pub fn extract_text_chunk(
    png_bytes: &[u8],
    target_keyword: &str,
) -> Result<Option<String>, MetadataError> {
    markits::raster::read_png_text_chunk(png_bytes, target_keyword)
        .map_err(|_| MetadataError::InvalidSignature)
}

/// Extract MarkIts annotations JSON from a PNG byte slice if present.
pub fn extract_annotations(png_bytes: &[u8]) -> Result<Option<String>, MetadataError> {
    extract_text_chunk(png_bytes, MARKITS_KEYWORD)
}

/// Extract MarkIts UI elements from a PNG byte slice if present.
pub fn extract_ui_elements(
    png_bytes: &[u8],
) -> Result<Option<Vec<crate::ui_elements::DetectedUiElement>>, MetadataError> {
    if let Some(json_str) = extract_text_chunk(png_bytes, MARKITS_UI_ELEMENTS_KEYWORD)? {
        let elements: Vec<crate::ui_elements::DetectedUiElement> = serde_json::from_str(&json_str)?;
        Ok(Some(elements))
    } else {
        Ok(None)
    }
}

/// Extract MarkIts crop information from a PNG byte slice if present.
pub fn extract_crop_info(png_bytes: &[u8]) -> Result<Option<CropInfo>, MetadataError> {
    if let Some(json_str) = extract_text_chunk(png_bytes, MARKITS_CROP_KEYWORD)? {
        let crop: CropInfo = serde_json::from_str(&json_str)?;
        Ok(Some(crop))
    } else {
        Ok(None)
    }
}

/// Embed or update MarkIts crop information into a PNG byte slice.
pub fn embed_crop_info(png_bytes: &[u8], crop_info: &CropInfo) -> Result<Vec<u8>, MetadataError> {
    let json_str = serde_json::to_string(crop_info)?;
    embed_text_chunk(png_bytes, MARKITS_CROP_KEYWORD, &json_str)
}

/// Remove any tEXt chunk with the given keyword from a PNG byte slice.
pub fn remove_text_chunk(png_bytes: &[u8], keyword: &str) -> Result<Vec<u8>, MetadataError> {
    if png_bytes.len() < 8 || &png_bytes[0..8] != PNG_SIGNATURE {
        return Err(MetadataError::InvalidSignature);
    }

    let mut output = Vec::with_capacity(png_bytes.len());
    output.extend_from_slice(PNG_SIGNATURE);

    let mut cursor = Cursor::new(&png_bytes[8..]);

    while (cursor.position() as usize) < cursor.get_ref().len() {
        let chunk_start_pos = 8 + cursor.position() as usize;

        let mut len_buf = [0u8; 4];
        if cursor.read_exact(&mut len_buf).is_err() {
            break;
        }
        let length = u32::from_be_bytes(len_buf) as usize;

        let mut type_buf = [0u8; 4];
        cursor.read_exact(&mut type_buf)?;
        if length
            .checked_add(4)
            .is_none_or(|needed| needed > cursor.get_ref().len() - cursor.position() as usize)
        {
            return Err(MetadataError::InvalidSignature);
        }

        let mut data = vec![0u8; length];
        cursor.read_exact(&mut data)?;

        let mut crc_buf = [0u8; 4];
        cursor.read_exact(&mut crc_buf)?;

        let total_chunk_len = 4 + 4 + length + 4;
        let original_chunk = &png_bytes[chunk_start_pos..chunk_start_pos + total_chunk_len];

        // Skip existing chunk with same keyword
        if &type_buf == b"tEXt" {
            if let Some(null_pos) = data.iter().position(|&b| b == 0) {
                if let Ok(kw) = std::str::from_utf8(&data[..null_pos]) {
                    if kw == keyword {
                        continue;
                    }
                }
            }
        }

        output.extend_from_slice(original_chunk);
    }

    Ok(output)
}

/// Embed or update a tEXt chunk into a PNG byte slice.
/// The chunk is inserted right after the IHDR chunk. Any existing chunk with the same keyword is replaced.
pub fn embed_text_chunk(
    png_bytes: &[u8],
    keyword: &str,
    text: &str,
) -> Result<Vec<u8>, MetadataError> {
    if png_bytes.len() < 8 || &png_bytes[0..8] != PNG_SIGNATURE {
        return Err(MetadataError::InvalidSignature);
    }

    let mut output = Vec::with_capacity(png_bytes.len() + text.len() + 64);
    output.extend_from_slice(PNG_SIGNATURE);

    // Build the new tEXt chunk
    let mut chunk_data = Vec::new();
    chunk_data.extend_from_slice(keyword.as_bytes());
    chunk_data.push(0u8);
    chunk_data.extend_from_slice(text.as_bytes());

    let chunk_len = u32::try_from(chunk_data.len())
        .map_err(|_| MetadataError::InvalidSignature)?
        .to_be_bytes();
    let chunk_type = b"tEXt";

    let mut hasher = Hasher::new();
    hasher.update(chunk_type);
    hasher.update(&chunk_data);
    let chunk_crc = hasher.finalize().to_be_bytes();

    let mut new_chunk = Vec::with_capacity(4 + 4 + chunk_data.len() + 4);
    new_chunk.extend_from_slice(&chunk_len);
    new_chunk.extend_from_slice(chunk_type);
    new_chunk.extend_from_slice(&chunk_data);
    new_chunk.extend_from_slice(&chunk_crc);

    let mut cursor = Cursor::new(&png_bytes[8..]);
    let mut inserted = false;

    while (cursor.position() as usize) < cursor.get_ref().len() {
        let chunk_start_pos = 8 + cursor.position() as usize;

        let mut len_buf = [0u8; 4];
        if cursor.read_exact(&mut len_buf).is_err() {
            break;
        }
        let length = u32::from_be_bytes(len_buf) as usize;

        let mut type_buf = [0u8; 4];
        cursor.read_exact(&mut type_buf)?;
        if length
            .checked_add(4)
            .is_none_or(|needed| needed > cursor.get_ref().len() - cursor.position() as usize)
        {
            return Err(MetadataError::InvalidSignature);
        }

        let mut data = vec![0u8; length];
        cursor.read_exact(&mut data)?;

        let mut crc_buf = [0u8; 4];
        cursor.read_exact(&mut crc_buf)?;

        let total_chunk_len = 4 + 4 + length + 4;
        let original_chunk = &png_bytes[chunk_start_pos..chunk_start_pos + total_chunk_len];

        // Skip existing chunk with same keyword
        if &type_buf == b"tEXt" {
            if let Some(null_pos) = data.iter().position(|&b| b == 0) {
                if let Ok(kw) = std::str::from_utf8(&data[..null_pos]) {
                    if kw == keyword {
                        continue;
                    }
                }
            }
        }

        output.extend_from_slice(original_chunk);

        // Insert new chunk immediately after IHDR
        if &type_buf == b"IHDR" && !inserted {
            output.extend_from_slice(&new_chunk);
            inserted = true;
        }
    }

    if !inserted {
        output.extend_from_slice(&new_chunk);
    }

    Ok(output)
}

/// Embed or update MarkIts annotations JSON into a PNG byte slice.
pub fn embed_annotations(
    png_bytes: &[u8],
    annotations_json: &str,
) -> Result<Vec<u8>, MetadataError> {
    embed_text_chunk(png_bytes, MARKITS_KEYWORD, annotations_json)
}

/// Embed or update MarkIts UI elements into a PNG byte slice.
pub fn embed_ui_elements(
    png_bytes: &[u8],
    elements: &[crate::ui_elements::DetectedUiElement],
) -> Result<Vec<u8>, MetadataError> {
    let json_str = serde_json::to_string(elements)?;
    embed_text_chunk(png_bytes, MARKITS_UI_ELEMENTS_KEYWORD, &json_str)
}

/// Embed annotations, UI elements, and optional crop info into a PNG byte slice.
pub fn embed_metadata(
    png_bytes: &[u8],
    annotations_json: Option<&str>,
    ui_elements: Option<&[crate::ui_elements::DetectedUiElement]>,
    crop_info: Option<&CropInfo>,
) -> Result<Vec<u8>, MetadataError> {
    let mut current = png_bytes.to_vec();
    if let Some(ann) = annotations_json {
        current = embed_annotations(&current, ann)?;
    }
    if let Some(els) = ui_elements {
        current = embed_ui_elements(&current, els)?;
    }
    if let Some(crop) = crop_info {
        current = embed_crop_info(&current, crop)?;
    } else {
        current = remove_text_chunk(&current, MARKITS_CROP_KEYWORD)?;
    }
    Ok(current)
}

/// Load an image from bytes, inspect for embedded MarkIts annotations, UI elements, and crop info, and return
/// the dimensions, Base64 data URL, and any restored metadata.
pub fn load_image_with_metadata(bytes: &[u8]) -> Result<LoadedImageResult, MetadataError> {
    let img = image::load_from_memory(bytes)?;
    let mut width = img.width();
    let mut height = img.height();

    // Check if it's a PNG and has embedded annotations, UI elements, or crop info
    let (annotations_json, ui_elements, crop_info) =
        if bytes.len() >= 8 && &bytes[0..8] == PNG_SIGNATURE {
            let ann = extract_annotations(bytes).unwrap_or(None);
            let uis = extract_ui_elements(bytes).unwrap_or(None);
            let crop = extract_crop_info(bytes).unwrap_or(None);
            (ann, uis, crop)
        } else {
            (None, None, None)
        };

    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mime = if bytes.len() >= 8 && &bytes[0..8] == PNG_SIGNATURE {
        "image/png"
    } else {
        "image/jpeg"
    };
    let mut image_data_url = format!("data:{};base64,{}", mime, b64);
    // Exported pixels already contain the marks. Restore the original background
    // before the editor draws the editable scene over it.
    if annotations_json.is_some() && mime == "image/png" {
        if let Some(source) = extract_text_chunk(bytes, MARKITS_SOURCE_KEYWORD)? {
            if let Some((header, payload)) = source.split_once(',') {
                if header.starts_with("data:image/") && header.ends_with(";base64") {
                    if let Ok(source_bytes) =
                        base64::engine::general_purpose::STANDARD.decode(payload)
                    {
                        if let Ok(source_image) = image::load_from_memory(&source_bytes) {
                            width = source_image.width();
                            height = source_image.height();
                            image_data_url = source;
                        }
                    }
                }
            }
        }
    }

    Ok(LoadedImageResult {
        width,
        height,
        image_data_url,
        annotations_json,
        history_id: None,
        ui_elements,
        base_image_data_url: None,
        base_width: None,
        base_height: None,
        base_ui_elements: None,
        crop_info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_chunk_length_larger_than_remaining_input() {
        let mut png = create_test_png();
        // First chunk is IHDR; make its declared length impossible.
        png[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(
            extract_text_chunk(&png, MARKITS_KEYWORD),
            Err(MetadataError::InvalidSignature)
        ));
    }
    use image::{ImageBuffer, Rgba};

    fn create_test_png() -> Vec<u8> {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(10, 10, Rgba([255, 0, 0, 255]));
        let mut buffer = Cursor::new(Vec::new());
        img.write_to(&mut buffer, image::ImageFormat::Png).unwrap();
        buffer.into_inner()
    }

    #[test]
    fn test_plain_png_has_no_annotations() {
        let png = create_test_png();
        let extracted = extract_annotations(&png).unwrap();
        assert_eq!(extracted, None);
    }

    #[test]
    fn test_embed_and_extract_annotations() {
        let png = create_test_png();
        let json_data = r#"{"canvas":{"width":10,"height":10},"annotations":[{"type":"arrow","target":[1,1,5,5]}]}"#;

        let embedded_png = embed_annotations(&png, json_data).unwrap();
        assert!(!embedded_png.is_empty());

        let extracted = extract_annotations(&embedded_png).unwrap();
        assert_eq!(extracted, Some(json_data.to_string()));

        // Ensure the PNG is still a valid image
        let loaded = image::load_from_memory(&embedded_png);
        assert!(loaded.is_ok());
        let img = loaded.unwrap();
        assert_eq!(img.width(), 10);
        assert_eq!(img.height(), 10);
    }

    #[test]
    fn test_re_embed_overwrites_existing_annotations() {
        let png = create_test_png();
        let initial_json = r#"{"version":1}"#;
        let updated_json = r#"{"version":2,"updated":true}"#;

        let png_v1 = embed_annotations(&png, initial_json).unwrap();
        assert_eq!(
            extract_annotations(&png_v1).unwrap(),
            Some(initial_json.to_string())
        );

        let png_v2 = embed_annotations(&png_v1, updated_json).unwrap();
        assert_eq!(
            extract_annotations(&png_v2).unwrap(),
            Some(updated_json.to_string())
        );
    }

    #[test]
    fn test_load_image_with_metadata_auto_detection() {
        let plain_png = create_test_png();
        let res_plain = load_image_with_metadata(&plain_png).unwrap();
        assert_eq!(res_plain.width, 10);
        assert_eq!(res_plain.height, 10);
        assert_eq!(res_plain.annotations_json, None);
        assert!(
            res_plain
                .image_data_url
                .starts_with("data:image/png;base64,")
        );

        let json_data = r#"{"canvas":{"width":10,"height":10},"annotations":[{"type":"rect","target":[0,0,5,5]}]}"#;
        let meta_png = embed_annotations(&plain_png, json_data).unwrap();
        let res_meta = load_image_with_metadata(&meta_png).unwrap();
        assert_eq!(res_meta.width, 10);
        assert_eq!(res_meta.height, 10);
        assert_eq!(res_meta.annotations_json, Some(json_data.to_string()));
    }

    #[test]
    fn test_inspect_png_header() {
        let plain_png = create_test_png();
        let info = inspect_png_header(&plain_png).unwrap();
        assert_eq!(info.width, 10);
        assert_eq!(info.height, 10);
        assert!(!info.has_annotations);
        assert_eq!(info.annotations_json, None);

        let json_data = r#"{"canvas":{"width":10,"height":10},"annotations":[{"type":"rect","target":[0,0,5,5]}]}"#;
        let meta_png = embed_annotations(&plain_png, json_data).unwrap();
        let info_meta = inspect_png_header(&meta_png).unwrap();
        assert_eq!(info_meta.width, 10);
        assert_eq!(info_meta.height, 10);
        assert!(info_meta.has_annotations);
        assert_eq!(info_meta.annotations_json, Some(json_data.to_string()));
    }

    #[test]
    fn test_embed_and_extract_ui_elements() {
        use crate::ui_elements::DetectedUiElement;

        let plain_png = create_test_png();
        let elements = vec![DetectedUiElement {
            role: "button".into(),
            name: Some("Submit".into()),
            window_id: None,
            pid: None,
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 30.0,
        }];

        let embedded = embed_ui_elements(&plain_png, &elements).unwrap();
        let extracted = extract_ui_elements(&embedded)
            .unwrap()
            .expect("should find ui elements");
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].role, "button");
        assert_eq!(extracted[0].name.as_deref(), Some("Submit"));
        assert_eq!(extracted[0].x, 10.0);

        // Also test combined embedding and load_image_with_metadata
        let combined =
            embed_metadata(&plain_png, Some(r#"{"test":true}"#), Some(&elements), None).unwrap();
        let loaded = load_image_with_metadata(&combined).unwrap();
        assert_eq!(loaded.annotations_json.as_deref(), Some(r#"{"test":true}"#));
        assert!(loaded.ui_elements.is_some());
        assert_eq!(loaded.ui_elements.unwrap().len(), 1);
        assert!(loaded.crop_info.is_none());
    }

    #[test]
    fn test_embed_and_extract_crop_info() {
        let plain_png = create_test_png();
        let crop = CropInfo {
            is_auto_cropped: true,
            offset_x: 15.0,
            offset_y: 25.0,
            base_width: 800,
            base_height: 600,
        };

        let embedded = embed_crop_info(&plain_png, &crop).unwrap();
        let extracted = extract_crop_info(&embedded)
            .unwrap()
            .expect("should find crop info");
        assert_eq!(extracted, crop);

        // Test inspect_png_header
        let info = inspect_png_header(&embedded).unwrap();
        assert_eq!(info.crop_info, Some(crop.clone()));

        // Test combined embed_metadata with crop_info
        let combined = embed_metadata(&plain_png, None, None, Some(&crop)).unwrap();
        let loaded = load_image_with_metadata(&combined).unwrap();
        assert_eq!(loaded.crop_info, Some(crop.clone()));

        // Test removal via embed_metadata with None
        let removed = embed_metadata(&combined, None, None, None).unwrap();
        let loaded_removed = load_image_with_metadata(&removed).unwrap();
        assert!(loaded_removed.crop_info.is_none());
    }
}
