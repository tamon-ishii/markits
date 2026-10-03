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
    assert!(
        stdout.contains("--detect-ui"),
        "Help should describe --detect-ui"
    );
    assert!(stdout.contains("--uimap"), "Help should describe --uimap");
    assert!(stdout.contains("--target"), "Help should describe --target");
    assert!(stdout.contains("--mark"), "Help should describe --mark");
}

#[test]
fn annotate_batch_applies_template_to_multiple_marks() {
    let dir = std::env::temp_dir().join(format!("markits_batch_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.png");
    let marks = dir.join("marks.json");
    let template = dir.join("template.json");
    let normal = dir.join("normal.png");
    let styled = dir.join("styled.png");
    let img = image::RgbaImage::new(80, 60);
    img.save(&source).unwrap();
    std::fs::write(&marks, r#"[{"type":"rect","target":[5,5,20,10]},{"type":"rect","target":[40,25,20,10]}]"#).unwrap();
    std::fs::write(&template, r#"{"style":"danger","stroke_width":7}"#).unwrap();
    for (output, use_template) in [(&normal, false), (&styled, true)] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_markits"));
        command.arg("annotate-batch").arg(&source).arg(&marks).arg("-o").arg(output);
        if use_template { command.arg("--template").arg(&template); }
        let result = command.output().unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    }
    assert_ne!(std::fs::read(normal).unwrap(), std::fs::read(styled).unwrap());
}

#[test]
fn annotate_warns_when_reused_uimap_dimensions_differ() {
    let dir = std::env::temp_dir().join(format!("markits_uimap_warning_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.png");
    let seed = dir.join("seed.png");
    let output = dir.join("output.png");
    image::RgbaImage::new(80, 60).save(&source).unwrap();
    let seed_image = image::RgbaImage::new(40, 30);
    let mut seed_png = Vec::new();
    seed_image.write_to(&mut std::io::Cursor::new(&mut seed_png), image::ImageFormat::Png).unwrap();
    let seed_png = raster::embed_png_uimap(&seed_png, &[UiElement::new("button", "Save", 5.0, 5.0, 15.0, 10.0)]).unwrap();
    std::fs::write(&seed, seed_png).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("annotate").arg(&source).arg("--uimap").arg(&seed)
        .args(["--target", "Save", "--mark", "rect", "-o"]).arg(&output)
        .output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(String::from_utf8_lossy(&result.stderr).contains("UIMap source is 40x30"));
}

#[test]
fn capture_rejects_ambiguous_region_options_before_accessing_display() {
    let binary = env!("CARGO_BIN_EXE_markits");
    let partial = Command::new(binary)
        .args(["capture", "/tmp/markits-invalid-region.png", "--x", "10"])
        .output()
        .unwrap();
    assert!(!partial.status.success());
    assert!(String::from_utf8_lossy(&partial.stderr).contains("must be provided together"));

    let conflicting = Command::new(binary)
        .args([
            "capture",
            "/tmp/markits-conflicting-source.png",
            "--screen",
            "1",
            "--window",
            "Editor",
        ])
        .output()
        .unwrap();
    assert!(!conflicting.status.success());
    assert!(String::from_utf8_lossy(&conflicting.stderr).contains("cannot be combined"));
}

#[test]
fn capture_series_rejects_invalid_count_before_accessing_display() {
    let output = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args(["capture-series", "/tmp/markits_series.png", "--count", "0"])
        .output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--count must be between"));
}

#[test]
fn test_annotate_with_png_uimap_and_target_box() {
    let tmp_dir = std::env::temp_dir();
    let base_png_path = tmp_dir.join("test_annotate_base.png");
    let out_png_path = tmp_dir.join("test_annotate_boxed.png");

    let img = image::RgbaImage::new(400, 300);
    let mut png_bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .unwrap();

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

    assert!(
        status.success(),
        "annotate with --uimap <png> and --target 保存ボタン --mark rect should succeed"
    );

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
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .unwrap();
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
        assert!(
            info.uimap.is_some(),
            "UIMap metadata should be embedded into captured PNG"
        );
        assert_eq!(info.uimap.unwrap()[0].name, "保存");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "Capture skipped or failed due to screen environment: {}",
            stderr
        );
    }
}

#[test]
fn test_capture_with_inline_box_annotation() {
    let tmp_dir = std::env::temp_dir();
    let out_path = tmp_dir.join("test_capture_inline_boxed.png");
    let base_uimap_path = tmp_dir.join("test_capture_seed_boxed.png");

    let img = image::RgbaImage::new(100, 100);
    let mut png_bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .unwrap();
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
        assert!(
            info.uimap.is_some(),
            "UIMap metadata should be preserved in annotated capture"
        );
        assert_eq!(info.uimap.unwrap()[0].name, "保存");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "Capture skipped or failed due to screen environment: {}",
            stderr
        );
    }
}

#[test]
fn test_cli_list_screens_and_windows() {
    let output_screens = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args(["capture", "--list-screens", "--json"])
        .output()
        .expect("Failed to run capture --list-screens");
    if output_screens.status.success() {
        let stdout = String::from_utf8_lossy(&output_screens.stdout);
        assert!(stdout.contains("scale_factor"));
    }

    let output_windows = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args(["capture", "--list-windows", "--json"])
        .output()
        .expect("Failed to run capture --list-windows");
    assert!(output_windows.status.success());
    let stdout = String::from_utf8_lossy(&output_windows.stdout);
    assert!(stdout.starts_with('['));
}

#[test]
fn test_cli_list_screens_and_windows_text_format() {
    let output_screens = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args(["capture", "--list-screens"])
        .output()
        .expect("Failed to run capture --list-screens");
    if output_screens.status.success() {
        let stdout = String::from_utf8_lossy(&output_screens.stdout);
        assert!(stdout.contains("Index"));
        assert!(stdout.contains("Resolution"));
    }

    let output_windows = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args(["capture", "--list-windows"])
        .output()
        .expect("Failed to run capture --list-windows");
    assert!(output_windows.status.success());
    let stdout = String::from_utf8_lossy(&output_windows.stdout);
    assert!(stdout.contains("Window ID"));
}

#[test]
fn test_cli_capture_screen_out_of_bounds() {
    let output = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args(["capture", "/tmp/dummy_screen_test.png", "--screen", "9999"])
        .output()
        .expect("Failed to run capture with screen out of bounds");
    assert!(!output.status.success(), "Screen out of bounds should fail");
}

#[test]
fn test_cli_capture_window_nonexistent() {
    let output = Command::new(env!("CARGO_BIN_EXE_markits"))
        .args([
            "capture",
            "/tmp/dummy_win_test.png",
            "--window",
            "NonExistentWindow9999999",
        ])
        .output()
        .expect("Failed to run capture with nonexistent window");
    assert!(
        !output.status.success(),
        "Nonexistent window query should fail"
    );
}
