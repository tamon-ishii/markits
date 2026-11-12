use markits::{
    capture_recipe::{CaptureRecipe, EmbeddedScript, LaunchSpec},
    recording::{OperationRecording, RecordedEvent},
};
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
fn ok(out: Output) -> Output {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}
fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn recipe(cwd: &Path) -> CaptureRecipe {
    CaptureRecipe {
        version: 1,
        launch: Some(LaunchSpec {
            program: None,
            script: Some(EmbeddedScript {
                name: "start.ps1".into(),
                content: "param($name)\nthrow 'must never execute during extraction'\n# 日本語\n"
                    .into(),
                runner: None,
            }),
            args: vec!["space \"quote\" ; $literal".into(), "--flag".into()],
            cwd: cwd.to_path_buf(),
        }),
        window: Some("Recipe Fixture".into()),
        window_bounds: None,
        timeout_ms: 1000,
        settle_ms: 0,
        include_cursor: false,
        ui_depth: 64,
        recording: Some(OperationRecording {
            version: 1,
            window: "Fixture".into(),
            bounds: [0, 0, 80, 60],
            events: vec![RecordedEvent::Screenshot {
                elapsed_ms: 0,
                include_cursor: false,
                pointer: None,
            }],
        }),
        capture_index: Some(0),
    }
}
fn image(path: &Path, recipe: &CaptureRecipe) {
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::new(80, 60)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    fs::write(
        path,
        markits::capture_recipe::embed(&png.into_inner(), recipe).unwrap(),
    )
    .unwrap();
}

#[test]
fn recipe_extracts_scripts_arguments_cwd_and_operations_without_execution_in_all_formats() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.png");
    let original = recipe(dir.path());
    image(&input, &original);
    for extension in ["png", "webp", "jxl"] {
        let encoded = dir.path().join(format!("recipe.{extension}"));
        ok(cli(&[
            "export",
            p(&input),
            "--editing-metadata",
            "-o",
            p(&encoded),
        ]));
        let out = ok(cli(&["recipe", p(&encoded)]));
        let info: serde_json::Value =
            serde_json::from_slice(&ok(cli(&["inspect", p(&encoded)])).stdout).unwrap();
        assert_eq!(info["has_capture_recipe"], true);
        let extracted: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(extracted, serde_json::to_value(&original).unwrap());
        let json = dir.path().join(format!("{extension}.json"));
        assert!(
            ok(cli(&["recipe", p(&encoded), "--output", p(&json)]))
                .stdout
                .is_empty()
        );
        let reused = ok(cli(&["capture-run", "--recipe", p(&json), "--dry-run"]));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&reused.stdout).unwrap(),
            extracted
        );
        let recapture = ok(cli(&["recapture", p(&encoded), "--dry-run"]));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&recapture.stdout).unwrap(),
            extracted
        );
        let clean = dir.path().join(format!("clean.{extension}"));
        ok(cli(&["strip-metadata", p(&encoded), "-o", p(&clean)]));
        assert!(!cli(&["recipe", p(&clean)]).status.success());
    }
    let annotated = dir.path().join("annotated.png");
    ok(cli(&[
        "annotate",
        p(&input),
        "--target",
        "[20,20,30,20]",
        "--mark",
        "rect",
        "--crop",
        "--editing-metadata",
        "-o",
        p(&annotated),
    ]));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&ok(cli(&["recipe", p(&annotated)])).stdout)
            .unwrap(),
        serde_json::to_value(original).unwrap()
    );
}

#[test]
fn capture_run_builds_self_contained_recipe_with_literal_arguments_and_rejects_invalid_operations()
{
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("startup.ps1");
    fs::write(&script, "# embedded source\nthrow 'do not run'\n").unwrap();
    let argument = "spaces \"quotes\" ; $(literal)";
    let out = ok(cli(&[
        "capture-run",
        "--launch-script",
        p(&script),
        "--arg",
        argument,
        "--arg=--flag",
        "--cwd",
        p(dir.path()),
        "--window",
        "Fixture",
        "--window-bounds",
        "[10,20,800,600]",
        "--dry-run",
    ]));
    let extracted: CaptureRecipe = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        extracted.launch.as_ref().unwrap().args,
        [argument, "--flag"]
    );
    assert!(
        extracted
            .launch
            .as_ref()
            .unwrap()
            .script
            .as_ref()
            .unwrap()
            .content
            .contains("embedded source")
    );
    fs::remove_file(script).unwrap();
    let image_path = dir.path().join("embedded.png");
    image(&image_path, &extracted);
    ok(cli(&["recapture", p(&image_path), "--dry-run"]));
    let bad = dir.path().join("bad.json");
    let mut invalid = serde_json::to_value(&extracted).unwrap();
    invalid["version"] = serde_json::json!(999);
    fs::write(&bad, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(
        !cli(&["capture-run", "--recipe", p(&bad), "--dry-run"])
            .status
            .success()
    );
    let operations = dir.path().join("operations.json");
    fs::write(&operations,r#"{"version":1,"window":"Fixture","bounds":[0,0,80,60],"events":[{"kind":"key","elapsed_ms":0,"value":"UnsupportedKey"}]}"#).unwrap();
    assert!(
        !cli(&[
            "capture-run",
            "--launch",
            "must-not-execute",
            "--window",
            "Fixture",
            "--operations",
            p(&operations),
            "--dry-run"
        ])
        .status
        .success()
    );
}
