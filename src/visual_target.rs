//! Small, immutable window-local templates used only when named controls are absent.
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{imageops::FilterType, RgbImage};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub png: String,
    pub anchor_x: f64,
    pub anchor_y: f64,
}

impl Target {
    pub fn decode(&self) -> Result<RgbImage, String> {
        if self.png.len() > 350_000 {
            return Err("画像照合用テンプレートが大きすぎます。".into());
        }
        let bytes = STANDARD.decode(&self.png).map_err(|e| e.to_string())?;
        let mut reader =
            image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(256);
        limits.max_image_height = Some(256);
        limits.max_alloc = Some(2_000_000);
        reader.limits(limits);
        let picture = reader.decode().map_err(|e| e.to_string())?.to_rgb8();
        if picture.width() < 16
            || picture.height() < 16
            || !self.anchor_x.is_finite()
            || !self.anchor_y.is_finite()
            || self.anchor_x < 0.0
            || self.anchor_y < 0.0
            || self.anchor_x >= picture.width() as f64
            || self.anchor_y >= picture.height() as f64
            || samples(&picture).len() < 16
        {
            return Err("画像照合には特徴のある画像と画像内のクリック位置が必要です。".into());
        }
        Ok(picture)
    }

    pub fn from_image(picture: &RgbImage, anchor_x: f64, anchor_y: f64) -> Result<Self, String> {
        let mut bytes = Cursor::new(Vec::new());
        picture
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        let target = Self {
            png: STANDARD.encode(bytes.into_inner()),
            anchor_x,
            anchor_y,
        };
        target.decode()?;
        Ok(target)
    }
}

/// Crop a pre-click snapshot of the selected window, never the desktop or a source screenshot.
pub fn record_snapshot(
    window: &super::window_capture::WindowInfo,
    picture: &RgbImage,
    x: f64,
    y: f64,
) -> Result<Target, String> {
    let rx = x - window.x as f64;
    let ry = y - window.y as f64;
    if window.width == 0
        || window.height == 0
        || rx < 0.0
        || ry < 0.0
        || rx >= window.width as f64
        || ry >= window.height as f64
    {
        return Err("クリックは撮影対象の外です。".into());
    }
    let sx = picture.width() as f64 / window.width as f64;
    let sy = picture.height() as f64 / window.height as f64;
    let left = (rx as u32).saturating_sub(48);
    let top = (ry as u32).saturating_sub(24);
    let px = (left as f64 * sx) as u32;
    let py = (top as f64 * sy) as u32;
    let width = ((96.min(window.width - left) as f64 * sx) as u32).min(picture.width() - px);
    let height = ((48.min(window.height - top) as f64 * sy) as u32).min(picture.height() - py);
    let patch = image::imageops::crop_imm(picture, px, py, width, height).to_image();
    Target::from_image(&patch, rx * sx - px as f64, ry * sy - py as f64)
}

fn difference(a: &[u8; 3], b: &[u8; 3]) -> u32 {
    a.iter().zip(b).map(|(a, b)| a.abs_diff(*b) as u32).sum()
}

// Sample distinct edges, spread throughout the patch; uniform backgrounds cannot identify controls.
fn samples(image: &RgbImage) -> Vec<(u32, u32)> {
    let mut edges = Vec::new();
    for y in 1..image.height() {
        for x in 1..image.width() {
            let pixel = image.get_pixel(x, y);
            if difference(&pixel.0, &image.get_pixel(x - 1, y).0) > 90
                || difference(&pixel.0, &image.get_pixel(x, y - 1).0) > 90
            {
                edges.push((x, y));
            }
        }
    }
    let step = (edges.len() / 48).max(1);
    edges.into_iter().step_by(step).take(48).collect()
}

/// Exhaustively search supported scales. No coordinates are used to choose between duplicates.
/// The callback bounds runtime and cancellation even during a large window search.
pub fn locate(
    target: &Target,
    window: &RgbImage,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<Option<(f64, f64)>, String> {
    let original = target.decode()?;
    let mut candidates: Vec<(f64, f64, f64, f64)> = Vec::new();
    for scale in [1.0, 0.8, 1.25, 1.5, 2.0, 0.5] {
        check()?;
        let width = (original.width() as f64 * scale).round() as u32;
        let height = (original.height() as f64 * scale).round() as u32;
        if width < 16 || height < 16 || width > window.width() || height > window.height() {
            continue;
        }
        let template = image::imageops::resize(&original, width, height, FilterType::Triangle);
        let points = samples(&template);
        if points.len() < 16 {
            continue;
        }
        for y in 0..=window.height() - height {
            if y % 8 == 0 {
                check()?;
            }
            for x in 0..=window.width() - width {
                let mut error = 0;
                let mut matches = true;
                for (index, &(tx, ty)) in points.iter().enumerate() {
                    let delta = difference(
                        &template.get_pixel(tx, ty).0,
                        &window.get_pixel(x + tx, y + ty).0,
                    );
                    error += delta;
                    if delta > 150 || error > (index as u32 + 1) * 36 {
                        matches = false;
                        break;
                    }
                }
                if !matches {
                    continue;
                }
                let mut total = 0u64;
                for ty in 0..height {
                    for tx in 0..width {
                        total += difference(
                            &template.get_pixel(tx, ty).0,
                            &window.get_pixel(x + tx, y + ty).0,
                        ) as u64;
                    }
                }
                let score = total as f64 / (width as f64 * height as f64 * 3.0);
                if score > 8.0 {
                    continue;
                }
                let px = x as f64 + target.anchor_x * width as f64 / original.width() as f64;
                let py = y as f64 + target.anchor_y * height as f64 / original.height() as f64;
                let radius = width.min(height) as f64 / 3.0;
                if let Some(existing) = candidates
                    .iter_mut()
                    .find(|old| (old.0 - px).hypot(old.1 - py) <= old.3.min(radius))
                {
                    if score < existing.2 {
                        *existing = (px, py, score, radius);
                    }
                } else {
                    candidates.push((px, py, score, radius));
                    if candidates.len() > 1 {
                        return Err(
                            "画像に一致する対象が複数あります。クリックを中止しました。".into()
                        );
                    }
                }
            }
        }
    }
    check()?;
    Ok(candidates.first().map(|hit| (hit.0, hit.1)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn patch() -> RgbImage {
        RgbImage::from_fn(32, 24, |x, y| {
            image::Rgb([
                ((x * 73 + y * 19) % 251) as u8,
                ((x * 17 + y * 61) % 253) as u8,
                ((x * 29 + y * 31) % 247) as u8,
            ])
        })
    }
    #[test]
    fn finds_moved_target_and_rejects_duplicates() {
        let patch = patch();
        let target = Target::from_image(&patch, 12.0, 10.0).unwrap();
        let mut window = RgbImage::from_pixel(120, 80, image::Rgb([240, 240, 240]));
        image::imageops::replace(&mut window, &patch, 65, 40);
        assert_eq!(
            locate(&target, &window, || Ok(())).unwrap(),
            Some((77.0, 50.0))
        );
        image::imageops::replace(&mut window, &patch, 10, 5);
        assert!(locate(&target, &window, || Ok(()))
            .unwrap_err()
            .contains("複数"));
    }
    #[test]
    fn supports_scale_changes_and_missing_targets() {
        let patch = patch();
        let target = Target::from_image(&patch, 12.0, 10.0).unwrap();
        let mut window = RgbImage::from_pixel(120, 80, image::Rgb([240, 240, 240]));
        assert!(locate(&target, &window, || Ok(())).unwrap().is_none());
        image::imageops::replace(
            &mut window,
            &image::imageops::resize(&patch, 48, 36, FilterType::Triangle),
            50,
            30,
        );
        assert_eq!(
            locate(&target, &window, || Ok(())).unwrap(),
            Some((68.0, 45.0))
        );
    }
    #[test]
    fn snapshot_anchor_tracks_window_coordinates_and_hidpi_pixels() {
        let mut picture = RgbImage::new(240, 160);
        image::imageops::replace(&mut picture, &patch(), 130, 80);
        let window = super::super::window_capture::WindowInfo {
            id: "fixture".into(),
            title: "fixture".into(),
            x: 400,
            y: 200,
            width: 120,
            height: 80,
        };
        let target = record_snapshot(&window, &picture, 472.0, 245.0).unwrap();
        let (x, y) = locate(&target, &picture, || Ok(())).unwrap().unwrap();
        assert_eq!((x, y), (144.0, 90.0));
        assert!(record_snapshot(&window, &picture, 399.0, 245.0).is_err());
    }
    #[test]
    fn rejects_blank_invalid_templates_and_honors_cancellation() {
        assert!(Target::from_image(&RgbImage::new(32, 24), 4.0, 5.0).is_err());
        assert!(Target {
            png: "invalid".into(),
            anchor_x: 0.0,
            anchor_y: 0.0
        }
        .decode()
        .is_err());
        assert!(Target::from_image(&patch(), f64::NAN, 5.0).is_err());
        let target = Target::from_image(&patch(), 5.0, 5.0).unwrap();
        assert_eq!(
            locate(&target, &RgbImage::new(120, 80), || Err("cancelled".into())).unwrap_err(),
            "cancelled"
        );
    }
}
