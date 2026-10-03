# Standalone Screen Capture, UIMap Reuse, and UI Targeting Implementation Plan

> **For Antigravity:** REQUIRED WORKFLOW: Use `.agent/workflows/execute-plan.md` to execute this plan in single-flow mode.

**Goal:** Provide `markits capture` for standalone screenshot capture with UI detection, enable UIMap reuse from PNG metadata, support UI element targeting (e.g., boxing "保存ボタン"), and add a prominent UI targeting guide to `README.md`.

**Architecture:** Expose screen capture and AT-SPI/X11 UI detection in the root `markits` crate. Implement `raster::load_uimap_from_path` supporting both `.png` (extracting embedded metadata) and `.json`. Implement `markits capture` in `src/main.rs` with `--detect-ui`, `--uimap`, and optional inline annotation (`--target`, `--mark`). Expand `README.md` with a major chapter on UI targeting.

**Tech Stack:** Rust 2024, `clap`, `screenshots`, `image`, `resvg`, `xa11y`, `x11-dl`, `zbus`.

---

### Task 1: Add dependencies & expose capture and UI elements in `markits`

**Files:**
- Modify: `Cargo.toml:20-30`
- Create: `src/capture.rs`
- Create: `src/ui_elements.rs`
- Modify: `src/lib.rs:1-20`
- Modify: `apps/desktop/src-tauri/src/capture.rs` (or re-export `markits::capture`)
- Modify: `apps/desktop/src-tauri/src/ui_elements.rs` (or re-export `markits::ui_elements`)

**Step 1: Write the failing test**
In `tests/capture_test.rs`:
```rust
#[test]
fn test_crop_rgba_buffer() {
    let mut img = image::RgbaImage::new(100, 100);
    let cropped = markits::capture::crop_rgba_image(&img, 10, 10, 20, 20).unwrap();
    assert_eq!(cropped.width(), 20);
    assert_eq!(cropped.height(), 20);
}
```

**Step 2: Run test to verify it fails**
Run: `cargo test --test capture_test`
Expected: FAIL (unresolved module `markits::capture`)

**Step 3: Implement minimal code**
- Add `screenshots = "0.8.10"`, `xa11y = { version = "0.15.1", default-features = false }` to root `Cargo.toml`.
- On Linux, add `x11-dl = "2.21"` and `zbus = { version = "5", features = ["blocking"] }`.
- Add `src/capture.rs` and `src/ui_elements.rs`, exporting them in `src/lib.rs`.
- In `src/ui_elements.rs`, convert `DetectedUiElement` to `markits::UiElement` cleanly.

**Step 4: Run test to verify it passes**
Run: `cargo test --test capture_test`
Expected: PASS

**Step 5: Commit**
```bash
git add Cargo.toml Cargo.lock src/capture.rs src/ui_elements.rs src/lib.rs tests/capture_test.rs
git commit -m "feat: expose screen capture and UI element detection in core markits"
```

---

### Task 2: Implement `load_uimap_from_path` in `src/raster.rs`

**Files:**
- Modify: `src/raster.rs`
- Test: `tests/uimap_reuse_test.rs`

**Step 1: Write the failing test**
In `tests/uimap_reuse_test.rs`:
```rust
use markits::{UiElement, raster};

#[test]
fn test_load_uimap_from_png_and_json() {
    let elements = vec![UiElement::new("button", "保存", 10.0, 20.0, 80.0, 30.0)];
    let tmp_dir = std::env::temp_dir();
    let json_path = tmp_dir.join("test_uimap.json");
    std::fs::write(&json_path, serde_json::to_string(&elements).unwrap()).unwrap();

    let loaded_json = raster::load_uimap_from_path(&json_path).unwrap();
    assert_eq!(loaded_json.len(), 1);
    assert_eq!(loaded_json[0].name, "保存");

    // Create a 10x10 PNG with embedded metadata
    let img = image::RgbaImage::new(10, 10);
    let mut png_bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).unwrap();
    let embedded = raster::embed_png_uimap(&png_bytes, &elements).unwrap();
    let png_path = tmp_dir.join("test_uimap.png");
    std::fs::write(&png_path, embedded).unwrap();

    let loaded_png = raster::load_uimap_from_path(&png_path).unwrap();
    assert_eq!(loaded_png.len(), 1);
    assert_eq!(loaded_png[0].name, "保存");
}
```

**Step 2: Run test to verify it fails**
Run: `cargo test --test uimap_reuse_test`
Expected: FAIL (`load_uimap_from_path` not found)

**Step 3: Implement `load_uimap_from_path` in `src/raster.rs`**
```rust
pub fn load_uimap_from_path(path: &Path) -> Result<Vec<UiElement>, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    // Check if it's a PNG by magic signature or format
    if bytes.starts_with(PNG_SIGNATURE) {
        if let Some(elements) = extract_png_uimap(&bytes) {
            return Ok(elements);
        }
        return Err(format!("No embedded UIMap found in PNG '{}'", path.display()).into());
    }
    // Otherwise parse as JSON
    let text = std::str::from_utf8(&bytes)?;
    let elements: Vec<UiElement> = serde_json::from_str(text)?;
    Ok(elements)
}
```

**Step 4: Run test to verify it passes**
Run: `cargo test --test uimap_reuse_test`
Expected: PASS

**Step 5: Commit**
```bash
git add src/raster.rs tests/uimap_reuse_test.rs
git commit -m "feat: support loading UIMap from both PNG metadata and JSON files"
```

---

### Task 3: Implement `markits capture` and update `--uimap` in `src/main.rs`

**Files:**
- Modify: `src/main.rs`
- Test: `tests/cli_capture_test.rs`

**Step 1: Write the failing test**
In `tests/cli_capture_test.rs`:
```rust
use std::process::Command;

#[test]
fn test_cli_capture_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_markits"))
        .arg("capture")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--detect-ui"));
    assert!(stdout.contains("--uimap"));
    assert!(stdout.contains("--target"));
}
```

**Step 2: Run test to verify it fails**
Run: `cargo test --test cli_capture_test`
Expected: FAIL (`capture` command does not exist)

**Step 3: Implement `Commands::Capture` and update `--uimap` handling in `src/main.rs`**
- In `Commands::Render`, `Commands::Validate`, `Commands::Annotate`:
  Replace `fs::read_to_string(uimap_path)` with `raster::load_uimap_from_path(&uimap_path)`.
- Define `Commands::Capture`:
  ```rust
  Capture {
      output: std::path::PathBuf,
      #[arg(long)]
      detect_ui: bool,
      #[arg(long)]
      uimap: Option<std::path::PathBuf>,
      #[arg(long)]
      x: Option<u32>,
      #[arg(long)]
      y: Option<u32>,
      #[arg(long)]
      width: Option<u32>,
      #[arg(long)]
      height: Option<u32>,
      #[arg(long)]
      target: Option<String>,
      #[arg(long, default_value = "rect")]
      mark: String,
      #[arg(long)]
      text: Option<String>,
      #[arg(long)]
      step: Option<u32>,
      #[arg(long, default_value = "primary")]
      style: String,
      #[arg(long, default_value = "auto")]
      position: String,
  }
  ```
- Implement handler:
  1. Capture primary screen (or region if x, y, width, height specified).
  2. If `uimap` is specified: load `effective_uimap` via `raster::load_uimap_from_path`.
  3. Else if `detect_ui` is specified: run desktop UI detection (`capture_desktop_detailed_elements`).
  4. Embed `effective_uimap` into PNG bytes.
  5. If `target` is specified: perform inline annotation using the captured image and resolved target (boxing/surrounding via `rect` or other marks).
  6. Write PNG to output path.

**Step 4: Run test to verify it passes**
Run: `cargo test --test cli_capture_test`
Expected: PASS

**Step 5: Commit**
```bash
git add src/main.rs tests/cli_capture_test.rs
git commit -m "feat: add markits capture CLI subcommand with UI detection and inline annotation"
```

---

### Task 4: Overhaul `README.md` and `docs/AI_MANUAL.md` with major UI targeting section

**Files:**
- Modify: `README.md`
- Modify: `docs/AI_MANUAL.md`

**Step 1: Write draft content in `README.md`**
Add a major section:
`# UI要素指定とAI自律連携 (UI Targeting & AI Automation)`
Covering:
1. `markits capture` with `--detect-ui` (automatic PNG metadata embedding).
2. UIMap reuse via `--uimap screen.png` (bypassing heavy AT-SPI rescanning).
3. "保存ボタンを囲む" example: `--target "保存ボタン" --mark rect`.
4. Command table and full example workflows for AI agents.

**Step 2: Update `docs/AI_MANUAL.md`**
Sync the new `capture` command, `--uimap <image.png>` capability, and box/rect workflows for AI agents.

**Step 3: Run all workspace tests**
Run: `cargo test --workspace`
Expected: All tests PASS.

**Step 4: Commit**
```bash
git add README.md docs/AI_MANUAL.md
git commit -m "docs: add prominent UI element targeting and AI automation section to README"
```
