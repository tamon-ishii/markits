use image::{ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_dir() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("markits-image-test-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn cli_composes_image_and_annotations_and_rejects_size_mismatch() {
    let dir = test_dir();
    let source = dir.join("source.png");
    let json = dir.join("annotations.json");
    let output = dir.join("annotated.png");
    RgbaImage::from_pixel(64, 48, Rgba([255, 255, 255, 255]))
        .save_with_format(&source, ImageFormat::Png)
        .unwrap();
    fs::write(&json, r##"{"canvas":{"width":64,"height":48},"annotations":[{"type":"rect","target":[10,10,20,20],"style":"primary"}]}"##).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args([
            "render",
            json.to_str().unwrap(),
            "--image",
            source.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    let rendered = image::open(&output).unwrap().to_rgba8();
    assert_eq!(rendered.dimensions(), (64, 48));
    assert_eq!(rendered.get_pixel(0, 0).0, [255, 255, 255, 255]);
    assert_ne!(rendered.get_pixel(10, 20).0, [255, 255, 255, 255]);

    fs::write(
        &json,
        r##"{"canvas":{"width":65,"height":48},"annotations":[]}"##,
    )
    .unwrap();
    let mismatch = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args([
            "render",
            json.to_str().unwrap(),
            "--image",
            source.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!mismatch.status.success());
    assert!(String::from_utf8_lossy(&mismatch.stderr).contains("do not match JSON canvas"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn help_points_to_bundled_manual() {
    let help = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("-h")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("markits manual"));

    let manual = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("manual")
        .output()
        .unwrap();
    assert!(manual.status.success());
    assert!(String::from_utf8_lossy(&manual.stdout).contains("MarkIts AI Manual"));
}

#[test]
fn cli_inspects_images_and_infers_canvas_for_render_and_validate() {
    let dir = test_dir();
    let source = dir.join("source.jpg");
    let json = dir.join("annotations.json");
    let output = dir.join("annotated.png");
    RgbImage::from_pixel(64, 48, Rgb([240, 240, 240]))
        .save_with_format(&source, ImageFormat::Jpeg)
        .unwrap();
    fs::write(
        &json,
        r##"{"annotations":[{"type":"rect","target":[10,10,20,20]}]}"##,
    )
    .unwrap();

    let inspect = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args(["inspect", source.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(inspect.status.success());
    let info: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(
        info,
        serde_json::json!({"width":64,"height":48,"format":"jpeg"})
    );

    let valid = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args([
            "validate",
            json.to_str().unwrap(),
            "--image",
            source.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );

    let rendered = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args([
            "render",
            json.to_str().unwrap(),
            "--image",
            source.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let image = image::open(&output).unwrap();
    assert_eq!((image.width(), image.height()), (64, 48));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn raster_output_contains_annotation_text() {
    let dir = test_dir();
    let source = dir.join("source.png");
    let json_path = dir.join("annotations.json");
    let output = dir.join("annotated.png");
    let json = r##"{"canvas":{"width":160,"height":100},"annotations":[{"type":"label","target":[60,60,40,20],"text":"X","position":"top","outline":false,"shadow":false}]}"##;
    RgbaImage::from_pixel(160, 100, Rgba([255, 255, 255, 255]))
        .save_with_format(&source, ImageFormat::Png)
        .unwrap();
    fs::write(&json_path, json).unwrap();
    let bounds = markits::render_with_layout_from_json(json)
        .unwrap()
        .elements[0]
        .bounds;

    let result = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args([
            "render",
            json_path.to_str().unwrap(),
            "--image",
            source.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let rendered = image::open(&output).unwrap().to_rgba8();
    let mut white_text_pixels = 0;
    for y in (bounds[1] as u32 + 5)..((bounds[1] + bounds[3]) as u32 - 5) {
        for x in (bounds[0] as u32 + 5)..((bounds[0] + bounds[2]) as u32 - 5) {
            let pixel = rendered.get_pixel(x, y);
            if pixel[0] > 220 && pixel[1] > 220 && pixel[2] > 220 {
                white_text_pixels += 1;
            }
        }
    }
    assert!(white_text_pixels > 0, "annotation text was not rasterized");
    fs::remove_dir_all(dir).unwrap();
}
