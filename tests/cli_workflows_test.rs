use markits::{UiElement, metadata, raster};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_markits"))
        .args(args)
        .output()
        .unwrap()
}
fn ok(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn source(path: &Path) {
    let image =
        image::RgbaImage::from_fn(160, 100, |x, y| image::Rgba([x as u8, y as u8, 120, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    let png = raster::embed_png_uimap(
        &out.into_inner(),
        &[UiElement::new("button", "Open", 70.0, 40.0, 40.0, 20.0)],
    )
    .unwrap();
    let png=metadata::embed_text_chunk(&png,markits::recording_metadata::OPERATIONS_KEY,r#"{"recording":{"version":1,"window":"Test","bounds":[0,0,160,100],"events":[]},"window_id":"0x123","pid":42}"#).unwrap();
    fs::write(path, png).unwrap();
}

#[test]
fn editable_crop_reopens_with_unpainted_background_and_translated_targets() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.png");
    source(&input);
    let output = dir.path().join("crop.png");
    ok(cli(&[
        "annotate",
        path(&input),
        "--target",
        "Open",
        "--mark",
        "rect",
        "--crop",
        "--crop-margin",
        "8",
        "--editing-metadata",
        "-o",
        path(&output),
    ]));
    let png = fs::read(&output).unwrap();
    let loaded = metadata::load_image_with_metadata(&png).unwrap();
    assert!(loaded.crop_info.is_some());
    assert!(loaded.base_image_data_url.is_some());
    assert!(
        metadata::extract_text_chunk(&png, markits::recording_metadata::OPERATIONS_KEY)
            .unwrap()
            .is_some()
    );
    let json = loaded.annotations_json.unwrap();
    let scene = markits::Scene::from_json(&json).unwrap();
    let crop = loaded.crop_info.unwrap();
    assert!(crop.offset_x > 0.0 && crop.offset_y > 0.0);
    let target = scene.annotations[0].target();
    assert_eq!(target.x, 70.0 - crop.offset_x);
    assert_eq!(target.y, 40.0 - crop.offset_y);
    use base64::Engine;
    let background = base64::engine::general_purpose::STANDARD
        .decode(loaded.image_data_url.split_once(',').unwrap().1)
        .unwrap();
    let clean = image::load_from_memory(&background).unwrap().to_rgba8();
    let original = image::open(&input).unwrap().to_rgba8();
    assert_eq!(
        clean.get_pixel(target.x as u32, target.y as u32),
        original.get_pixel(70, 40)
    );
    let repainted = markits::render_composed_png_bytes(&json, &background).unwrap();
    assert_eq!(
        image::load_from_memory(&repainted).unwrap().to_rgba8(),
        image::load_from_memory(&png).unwrap().to_rgba8()
    );
    let recrop = dir.path().join("recrop.png");
    let dims = image::load_from_memory(&png).unwrap();
    ok(cli(&[
        "crop",
        path(&output),
        "--x",
        "1",
        "--y",
        "1",
        "--width",
        &(dims.width() - 2).to_string(),
        "--height",
        &(dims.height() - 2).to_string(),
        "--editing-metadata",
        "--output",
        path(&recrop),
    ]));
    let reloaded = metadata::load_image_with_metadata(&fs::read(recrop).unwrap()).unwrap();
    assert_eq!(reloaded.crop_info.unwrap().offset_x, crop.offset_x + 1.0);
    assert!(reloaded.annotations_json.is_some());
}

#[test]
fn lossless_formats_roundtrip_editing_payload_and_flatten_on_export() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.png");
    source(&input);
    for extension in ["webp", "jxl"] {
        let encoded = dir.path().join(format!("editable.{extension}"));
        ok(cli(&[
            "export",
            path(&input),
            "--editing-metadata",
            "-o",
            path(&encoded),
        ]));
        let restored =
            markits::image_export::to_editable_png(&fs::read(&encoded).unwrap()).unwrap();
        assert_eq!(
            image::load_from_memory(&restored).unwrap().to_rgba8(),
            image::open(&input).unwrap().to_rgba8()
        );
        assert_eq!(
            raster::extract_png_uimap(&restored).unwrap()[0].name,
            "Open"
        );
        ok(cli(&["inspect", path(&encoded), "--uimap"]));
        let detail = dir.path().join(format!("detail-{extension}.png"));
        ok(cli(&[
            "annotate",
            path(&encoded),
            "--target",
            "Open",
            "--mark",
            "rect",
            "-o",
            path(&detail),
        ]));
        let flattened = dir.path().join(format!("flat-{extension}.png"));
        ok(cli(&["export", path(&encoded), "-o", path(&flattened)]));
        let flat = fs::read(flattened).unwrap();
        assert!(raster::extract_png_uimap(&flat).is_none());
        assert!(
            metadata::extract_text_chunk(&flat, markits::recording_metadata::OPERATIONS_KEY)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn history_roundtrip_uses_desktop_format_and_rejects_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.png");
    source(&input);
    let history = dir.path().join("history");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_markits"))
            .env("MARKITS_HISTORY_DIR", &history)
            .args(args)
            .output()
            .unwrap()
    };
    let saved = ok(run(&["history", "save", path(&input)]));
    let item: serde_json::Value = serde_json::from_slice(&saved.stdout).unwrap();
    let id = item["id"].as_str().unwrap();
    let listing = ok(run(&["history", "list"]));
    assert!(String::from_utf8_lossy(&listing.stdout).contains(id));
    let output = dir.path().join("loaded.png");
    ok(run(&["history", "load", id, "-o", path(&output)]));
    assert!(
        metadata::extract_text_chunk(
            &fs::read(output).unwrap(),
            markits::recording_metadata::OPERATIONS_KEY
        )
        .unwrap()
        .is_some()
    );
    assert!(!run(&["history", "delete", "../source"]).status.success());
    assert!(input.exists());
    ok(run(&["history", "delete", id]));
    let listing = ok(run(&["history", "list"]));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&listing.stdout).unwrap(),
        serde_json::json!([])
    );
}

#[test]
fn replay_dry_run_reads_json_and_embedded_operations_without_desktop_access() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.png");
    source(&input);
    ok(cli(&["replay", path(&input), "--dry-run"]));
    let recording = dir.path().join("recording.json");
    fs::write(&recording,r#"{"version":1,"window":"Absent","bounds":[0,0,160,100],"events":[{"kind":"key","elapsed_ms":0,"value":"Control+S"},{"kind":"screenshot","elapsed_ms":20}]}"#).unwrap();
    ok(cli(&["replay", path(&recording), "--dry-run"]));
    fs::write(&recording,r#"{"version":1,"window":"Absent","bounds":[0,0,160,100],"events":[{"kind":"click","elapsed_ms":0,"x":999,"y":5}]}"#).unwrap();
    assert!(
        !cli(&["replay", path(&recording), "--dry-run"])
            .status
            .success()
    );
    assert!(!cli(&["capture", "--ui-depth", "129"]).status.success());
}
