use base64::Engine;
use image::{DynamicImage, ImageFormat};
use markits::{Scene, render_from_json};
use resvg::{tiny_skia, usvg};
use std::error::Error;
use std::fs;
use std::path::Path;

pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: &'static str,
}

fn read_image(path: &Path) -> Result<(Vec<u8>, ImageInfo), Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let (format, name) = match image::guess_format(&bytes)? {
        ImageFormat::Png => (ImageFormat::Png, "png"),
        ImageFormat::Jpeg => (ImageFormat::Jpeg, "jpeg"),
        _ => return Err("Image must contain a PNG or JPEG image".into()),
    };
    let source = image::load_from_memory_with_format(&bytes, format)?;
    Ok((
        bytes,
        ImageInfo {
            width: source.width(),
            height: source.height(),
            format: name,
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
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    let root = value
        .as_object_mut()
        .ok_or("Annotation JSON root must be an object")?;
    root.entry("canvas")
        .or_insert_with(|| serde_json::json!({"width": width, "height": height}));
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

pub fn render_png(json: &str, image_path: &Path, output_path: &Path) -> Result<(), Box<dyn Error>> {
    if !output_path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        return Err("--output must be a .png file".into());
    }

    let (image_bytes, info) = read_image(image_path)?;
    let resolved = with_image_canvas(json, info.width, info.height)?;
    let scene = Scene::from_json(&resolved)?;
    ensure_canvas_matches(&scene, &info)?;

    let svg = render_from_json(&resolved)?;
    let root_end = svg.find('>').ok_or("Generated SVG has no root element")? + 1;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
    let mut composed = String::with_capacity(svg.len() + encoded.len() + 200);
    composed.push_str(&svg[..root_end]);
    composed.push_str(&format!(
        "\n  <image x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" href=\"data:image/{};base64,{}\"/>",
        info.width, info.height, info.format, encoded
    ));
    composed.push_str(&svg[root_end..]);

    let mut options = usvg::Options::default();
    let fonts = options.fontdb_mut();
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
    .ok_or("No system fonts are available for rendering annotation text")?;
    options.font_family = font_family;
    let rendered_font = format!(
        "font-family=\"{}\"",
        markits::renderer::escape_xml(&options.font_family)
    );
    let composed = composed.replace(
        &format!("font-family=\"{}\"", markits::Theme::default().font_family),
        &rendered_font,
    );
    let tree = usvg::Tree::from_str(&composed, &options)?;
    let mut pixmap = tiny_skia::Pixmap::new(info.width, info.height)
        .ok_or("Image dimensions are too large to render")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(output_path)?;
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
}
