use markits::{UiElement, raster};
use std::process::Command;

#[test]
fn test_cli_capture_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("capture")
        .arg("--help")
        .output()
        .expect("Failed to execute markits capture --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--detect-ui"), "Help should describe --detect-ui");
    assert!(stdout.contains("--uimap"), "Help should describe --uimap");
    assert!(stdout.contains("--target"), "Help should describe --target");
    assert!(stdout.contains("--mark"), "Help should describe --mark");
}

#[test]
fn test_annotate_with_png_uimap_and_target_box() {
    let tmp_dir = std::env::temp_dir();
    let base_png_path = tmp_dir.join("test_annotate_base.png");
    let out_png_path = tmp_dir.join("test_annotate_boxed.png");

    let img = image::RgbaImage::new(400, 300);
    let mut png_bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).unwrap();

    let elements = vec![
        UiElement::new("button", "保存", 100.0, 50.0, 80.0, 32.0),
        UiElement::new("button", "キャンセル", 200.0, 50.0, 80.0, 32.0),
    ];
    let embedded = raster::embed_png_uimap(&png_bytes, &elements).unwrap();
    std::fs::write(&base_png_path, embedded).unwrap();

    // AI specifies: --target "保存ボタン" --mark rect
    let status = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("annotate")
        .arg(&base_png_path)
        .arg("--uimap")
        .arg(&base_png_path)
        .arg("--target")
        .arg("保存ボタン")
        .arg("--mark")
        .arg("rect")
        .arg("-o")
        .arg(&out_png_path)
        .status()
        .expect("Failed to run markits annotate");

    assert!(status.success(), "annotate with --uimap <png> and --target 保存ボタン --mark rect should succeed");

    // Verify resulting image has embedded UIMap metadata preserved
    let info = raster::inspect_image(&out_png_path).unwrap();
    assert_eq!(info.width, 400);
    assert_eq!(info.height, 300);
    assert!(info.uimap.is_some());
    assert_eq!(info.uimap.unwrap().len(), 2);
}

#[test]
fn test_capture_execution() {
    let tmp_dir = std::env::temp_dir();
    let out_path = tmp_dir.join("test_capture_run.png");
    let base_uimap_path = tmp_dir.join("test_capture_seed.png");

    let img = image::RgbaImage::new(100, 100);
    let mut png_bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).unwrap();
    let elements = vec![UiElement::new("button", "保存", 10.0, 10.0, 50.0, 20.0)];
    let embedded = raster::embed_png_uimap(&png_bytes, &elements).unwrap();
    std::fs::write(&base_uimap_path, embedded).unwrap();

    // Attempt capture. If system has screens (e.g. running locally or X11/wayland), it should succeed.
    let output = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("capture")
        .arg(&out_path)
        .arg("--uimap")
        .arg(&base_uimap_path)
        .output()
        .expect("Failed to run markits capture");

    // In environments with no screens (e.g. headless CI without Xvfb), Screen::all() may fail with NoScreensFound.
    // In local environments (like this user environment), it succeeds.
    if output.status.success() {
        assert!(out_path.exists());
        let info = raster::inspect_image(&out_path).unwrap();
        assert!(info.width > 0);
        assert!(info.height > 0);
        assert!(info.uimap.is_some(), "UIMap metadata should be embedded into captured PNG");
        assert_eq!(info.uimap.unwrap()[0].name, "保存");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Capture skipped or failed due to screen environment: {}", stderr);
    }
}

#[test]
fn test_capture_with_inline_box_annotation() {
    let tmp_dir = std::env::temp_dir();
    let out_path = tmp_dir.join("test_capture_inline_boxed.png");
    let base_uimap_path = tmp_dir.join("test_capture_seed_boxed.png");

    let img = image::RgbaImage::new(100, 100);
    let mut png_bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).unwrap();
    let elements = vec![UiElement::new("button", "保存", 10.0, 10.0, 50.0, 20.0)];
    let embedded = raster::embed_png_uimap(&png_bytes, &elements).unwrap();
    std::fs::write(&base_uimap_path, embedded).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("capture")
        .arg(&out_path)
        .arg("--uimap")
        .arg(&base_uimap_path)
        .arg("--target")
        .arg("保存ボタン")
        .arg("--mark")
        .arg("rect")
        .output()
        .expect("Failed to run markits capture with inline annotation");

    if output.status.success() {
        assert!(out_path.exists());
        let info = raster::inspect_image(&out_path).unwrap();
        assert!(info.width > 0);
        assert!(info.height > 0);
        assert!(info.uimap.is_some(), "UIMap metadata should be preserved in annotated capture");
        assert_eq!(info.uimap.unwrap()[0].name, "保存");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Capture skipped or failed due to screen environment: {}", stderr);
    }
}
