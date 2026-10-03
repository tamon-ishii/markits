use base64::Engine;
use image::{imageops, RgbaImage};
use screenshots::Screen;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("No screens detected on system")]
    NoScreensFound,
    #[error("Screen capture failed: {0}")]
    CaptureFailed(String),
    #[error("Image encoding failed: {0}")]
    ImageEncode(#[from] image::ImageError),
    #[error("Invalid capture region: width and height must be > 0")]
    InvalidRegion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    pub data_url: String,
    #[serde(skip_serializing, default)]
    pub raw_png: Vec<u8>,
    #[serde(default)]
    pub ui_elements: Vec<crate::ui_elements::DetectedUiElement>,
}

impl CapturedImage {
    pub fn to_ui_elements(&self) -> Vec<crate::UiElement> {
        self.ui_elements.iter().map(|el| el.to_ui_element()).collect()
    }
}

/// Convert an RgbaImage to PNG bytes and Base64 data URL.
pub fn rgba_to_captured_image(img: &RgbaImage) -> Result<CapturedImage, CaptureError> {
    let mut cursor = Cursor::new(Vec::new());
    img.write_to(&mut cursor, image::ImageFormat::Png)?;
    let raw_png = cursor.into_inner();

    let b64 = base64::engine::general_purpose::STANDARD.encode(&raw_png);
    let data_url = format!("data:image/png;base64,{}", b64);

    Ok(CapturedImage {
        width: img.width(),
        height: img.height(),
        data_url,
        raw_png,
        ui_elements: Vec::new(),
    })
}

/// Crop an RgbaImage safely to the specified bounds.
pub fn crop_rgba_image(img: &RgbaImage, x: u32, y: u32, width: u32, height: u32) -> Result<RgbaImage, CaptureError> {
    if width == 0 || height == 0 {
        return Err(CaptureError::InvalidRegion);
    }

    let img_w = img.width();
    let img_h = img.height();

    let crop_x = x.min(img_w);
    let crop_y = y.min(img_h);
    let crop_w = width.min(img_w.saturating_sub(crop_x));
    let crop_h = height.min(img_h.saturating_sub(crop_y));

    if crop_w == 0 || crop_h == 0 {
        return Err(CaptureError::InvalidRegion);
    }

    let cropped = imageops::crop_imm(img, crop_x, crop_y, crop_w, crop_h).to_image();
    Ok(cropped)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScreenInfo {
    pub index: usize,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

/// Enumerate all connected screens.
pub fn list_screens() -> Result<Vec<ScreenInfo>, CaptureError> {
    let screens = Screen::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    if screens.is_empty() {
        return Err(CaptureError::NoScreensFound);
    }
    let mut list = Vec::new();
    for (i, s) in screens.iter().enumerate() {
        list.push(ScreenInfo {
            index: i,
            name: format!("Screen {}", i),
            x: s.display_info.x,
            y: s.display_info.y,
            width: s.display_info.width,
            height: s.display_info.height,
            scale_factor: s.display_info.scale_factor as f64,
            is_primary: s.display_info.is_primary,
        });
    }
    Ok(list)
}

/// Capture a specific screen by index.
pub fn capture_screen(screen_index: usize) -> Result<CapturedImage, CaptureError> {
    let screens = Screen::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    let screen = screens.get(screen_index).ok_or_else(|| {
        CaptureError::CaptureFailed(format!("Screen index {} not found (total screens: {})", screen_index, screens.len()))
    })?;

    let raw_sc = screen.capture().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    let width = raw_sc.width();
    let height = raw_sc.height();
    let raw_bytes = raw_sc.into_raw();

    let image = RgbaImage::from_raw(width, height, raw_bytes)
        .ok_or_else(|| CaptureError::CaptureFailed("Failed to construct image from screen buffer".to_string()))?;

    rgba_to_captured_image(&image)
}

/// Capture the primary screen (or first detected screen).
pub fn capture_primary_screen() -> Result<CapturedImage, CaptureError> {
    let screens = Screen::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    let primary_idx = screens
        .iter()
        .position(|s| s.display_info.is_primary)
        .unwrap_or(0);
    capture_screen(primary_idx)
}

/// Capture a specific rectangular region of the primary screen.
pub fn capture_region(x: u32, y: u32, width: u32, height: u32) -> Result<CapturedImage, CaptureError> {
    if width == 0 || height == 0 {
        return Err(CaptureError::InvalidRegion);
    }

    let screens = Screen::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    let screen = screens.first().ok_or(CaptureError::NoScreensFound)?;

    let raw_sc = screen.capture().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    let sc_w = raw_sc.width();
    let sc_h = raw_sc.height();
    let raw_bytes = raw_sc.into_raw();

    let full_image = RgbaImage::from_raw(sc_w, sc_h, raw_bytes)
        .ok_or_else(|| CaptureError::CaptureFailed("Failed to construct image from screen buffer".to_string()))?;

    let cropped = crop_rgba_image(&full_image, x, y, width, height)?;
    rgba_to_captured_image(&cropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    #[test]
    fn test_crop_rgba_image_boundaries() {
        let mut img: RgbaImage = ImageBuffer::new(100, 100);
        img.put_pixel(10, 10, Rgba([255, 0, 0, 255]));
        img.put_pixel(20, 20, Rgba([0, 255, 0, 255]));

        let cropped = crop_rgba_image(&img, 10, 10, 20, 20).unwrap();
        assert_eq!(cropped.width(), 20);
        assert_eq!(cropped.height(), 20);
        assert_eq!(*cropped.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
        assert_eq!(*cropped.get_pixel(10, 10), Rgba([0, 255, 0, 255]));

        // Test clamping to boundary
        let clamped = crop_rgba_image(&img, 90, 90, 50, 50).unwrap();
        assert_eq!(clamped.width(), 10);
        assert_eq!(clamped.height(), 10);

        // Test invalid dimensions
        assert!(crop_rgba_image(&img, 0, 0, 0, 10).is_err());
        assert!(crop_rgba_image(&img, 100, 100, 10, 10).is_err());
    }

    #[test]
    fn test_rgba_to_captured_image() {
        let img: RgbaImage = ImageBuffer::new(4, 4);
        let captured = rgba_to_captured_image(&img).unwrap();
        assert_eq!(captured.width, 4);
        assert_eq!(captured.height, 4);
        assert!(captured.data_url.starts_with("data:image/png;base64,"));
        assert!(!captured.raw_png.is_empty());
    }

    #[test]
    fn test_list_screens_returns_valid_info() {
        let screens = list_screens().expect("list_screens should succeed");
        assert!(!screens.is_empty(), "should detect at least one screen");
        let primary = screens.iter().find(|s| s.is_primary).or(screens.first()).unwrap();
        assert!(primary.width > 0);
        assert!(primary.height > 0);
        assert!(primary.scale_factor > 0.0);
    }

    #[test]
    fn test_capture_screen_out_of_bounds_fails() {
        let result = capture_screen(9999);
        assert!(result.is_err());
    }
}
