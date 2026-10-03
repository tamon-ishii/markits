use markits::{UiElement, raster};
use std::process::Command;

#[test]
fn test_annotate_with_crop_reduces_dimensions_and_translates_uimap() {
    let tmp_dir = std::env::temp_dir();
    let base_png_path = tmp_dir.join("test_crop_base.png");
    let out_png_path = tmp_dir.join("test_crop_out.png");

    let img = image::RgbaImage::new(400, 300);
    let mut png_bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).unwrap();

    let elements = vec![
        UiElement::new("button", "保存", 100.0, 100.0, 80.0, 30.0),
        UiElement::new("button", "設定", 300.0, 250.0, 50.0, 20.0),
    ];
    let embedded = raster::embed_png_uimap(&png_bytes, &elements).unwrap();
    std::fs::write(&base_png_path, embedded).unwrap();

    // Run annotate with --crop and --crop-margin 10
    let status = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("annotate")
        .arg(&base_png_path)
        .arg("--target")
        .arg("保存")
        .arg("--mark")
        .arg("rect")
        .arg("--crop")
        .arg("--crop-margin")
        .arg("10")
        .arg("-o")
        .arg(&out_png_path)
        .status()
        .expect("Failed to run markits annotate with --crop");

    assert!(status.success(), "annotate with --crop should succeed");

    // Verify dimensions:
    // Target is [100, 100, 80, 30].
    // Margin is 10.
    // crop_x = 100 - 10 = 90, crop_y = 100 - 10 = 90
    // crop_w = 80 + 20 = 100, crop_h = 30 + 20 = 50
    let info = raster::inspect_image(&out_png_path).unwrap();
    assert_eq!(info.width, 100);
    assert_eq!(info.height, 50);

    // Verify UIMap translated coordinates
    assert!(info.uimap.is_some(), "Cropped image should keep UIMap");
    let cropped_elements = info.uimap.unwrap();
    assert_eq!(cropped_elements.len(), 1, "Only element within crop should remain");
    assert_eq!(cropped_elements[0].name, "保存");
    assert_eq!(cropped_elements[0].x, 10.0);
    assert_eq!(cropped_elements[0].y, 10.0);
    assert_eq!(cropped_elements[0].width, 80.0);
    assert_eq!(cropped_elements[0].height, 30.0);
}

#[test]
fn test_render_with_crop_and_default_margin() {
    let tmp_dir = std::env::temp_dir();
    let base_png_path = tmp_dir.join("test_render_crop_base.png");
    let layout_json_path = tmp_dir.join("test_render_crop_layout.json");
    let out_png_path = tmp_dir.join("test_render_crop_out.png");

    let img = image::RgbaImage::new(500, 400);
    let mut png_bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).unwrap();
    std::fs::write(&base_png_path, png_bytes).unwrap();

    let json = r#"{
        "annotations": [
            {
                "type": "rect",
                "target": [200, 150, 100, 60]
            }
        ]
    }"#;
    std::fs::write(&layout_json_path, json).unwrap();

    // Default crop-margin is 32px
    let status = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("render")
        .arg(&layout_json_path)
        .arg("--image")
        .arg(&base_png_path)
        .arg("--crop")
        .arg("-o")
        .arg(&out_png_path)
        .status()
        .expect("Failed to run markits render with --crop");

    assert!(status.success(), "render with --crop should succeed");

    let info = raster::inspect_image(&out_png_path).unwrap();
    // Target is [200, 150, 100, 60]
    // Default margin 32:
    // crop_x = 200 - 32 = 168, crop_y = 150 - 32 = 118
    // crop_w = 100 + 64 = 164, crop_h = 60 + 64 = 124
    assert_eq!(info.width, 164);
    assert_eq!(info.height, 124);
}
