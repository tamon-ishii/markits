# Auto-Crop to Annotation Bounds Implementation Plan

> **For Antigravity:** REQUIRED WORKFLOW: Use `.agent/workflows/execute-plan.md` to execute this plan in single-flow mode.

**Goal:** Implement auto-cropping to the bounding box of placed annotations with configurable margin in MarkIts CLI and Desktop GUI, maintaining translated UIMap metadata.

**Architecture:** Extend `src/raster.rs` with annotation bounding box computation and auto-crop rendering (`render_composed_png_bytes_with_crop`), add `--crop` and `--crop-margin <PX>` options across `annotate`, `render`, and `capture` in `src/main.rs`, translate embedded UIMap coordinates relative to the crop origin, and add a "マークに合わせてクロップ" action in Desktop GUI (`apps/desktop`).

**Tech Stack:** Rust (resvg, tiny-skia, image-rs, clap, serde), TypeScript / HTML5 Canvas (Tauri Desktop App).

---

### Task 1: Implement Bounding Box & Auto-Crop Rendering in `src/raster.rs`

**Files:**
- Modify: `src/raster.rs`
- Test: `src/raster.rs` (unit tests)

**Step 1: Write failing unit test in `src/raster.rs`**
Add tests verifying bounding box calculation from `LayoutElement` items and composed PNG cropping.

```rust
#[test]
fn test_compute_annotation_bounds() {
    use crate::renderer::LayoutElement;
    let elements = vec![
        LayoutElement {
            id: "el1".to_string(),
            bounds: [100.0, 50.0, 80.0, 30.0],
            arrow_path: None,
        },
        LayoutElement {
            id: "el2".to_string(),
            bounds: [120.0, 100.0, 40.0, 20.0],
            arrow_path: Some(vec![[150.0, 150.0]]),
        },
    ];
    let bounds = compute_annotation_bounds(&elements).expect("Should compute bounds");
    assert_eq!(bounds, [100.0, 50.0, 60.0, 100.0]); // min_x: 100, min_y: 50, max_x: 160, max_y: 150
}
```

**Step 2: Run test to verify it fails**
Run: `cargo test --lib raster::tests::test_compute_annotation_bounds`
Expected: FAIL ("cannot find function `compute_annotation_bounds`")

**Step 3: Implement minimal code in `src/raster.rs`**
Implement `compute_annotation_bounds`, `filter_ui_elements_for_crop`, and `render_composed_png_bytes_with_crop`.
- `compute_annotation_bounds` takes `&[LayoutElement]`, iterates over `bounds` and `arrow_path`, returns `Option<[f64; 4]>` (`[min_x, min_y, width, height]`).
- `filter_ui_elements_for_crop` filters and offsets `&[UiElement]` relative to `(crop_x, crop_y)`.
- `render_composed_png_bytes_with_crop` calls `renderer::render_with_layout_from_json(&resolved)?`, renders pixmap, and if `crop_margin` is `Some(margin)` and bounds exist, crops the image and filters UIMap.
- Keep `render_composed_png_bytes(json, image_bytes)` delegating to `render_composed_png_bytes_with_crop(json, image_bytes, None)`.

**Step 4: Run test to verify it passes**
Run: `cargo test --lib raster::tests`
Expected: PASS

**Step 5: Commit**
```bash
git add src/raster.rs
git commit -m "feat(raster): add annotation bounds calculation and auto-crop rendering"
```

---

### Task 2: Add `--crop` and `--crop-margin` CLI Flags to `src/main.rs`

**Files:**
- Modify: `src/main.rs`

**Step 1: Write CLI args in `src/main.rs`**
Add `--crop` and `--crop-margin` (default `32`) to:
1. `Commands::Render`
2. `Commands::Annotate`
3. `Commands::Capture`

**Step 2: Wire execution logic in `src/main.rs`**
- In `Commands::Render`: Pass `if crop { Some(crop_margin) } else { None }` to `raster::render_composed_png_bytes_with_crop`.
- In `Commands::Annotate`: Pass `if crop { Some(crop_margin) } else { None }` to `raster::render_composed_png_bytes_with_crop`.
- In `Commands::Capture`:
  If `crop` is enabled and `target.is_some()`, pass `Some(crop_margin)` to `raster::render_composed_png_bytes_with_crop`. If `target.is_none()`, warn if `--crop` is passed and save full image.

**Step 3: Verify CLI compiles and `--help` displays options**
Run:
```bash
cargo run --bin markits -- annotate --help | grep crop
cargo run --bin markits -- render --help | grep crop
cargo run --bin markits -- capture --help | grep crop
```
Expected: PASS (each command lists `--crop` and `--crop-margin`)

**Step 4: Commit**
```bash
git add src/main.rs
git commit -m "feat(cli): add --crop and --crop-margin flags to annotate, render, and capture"
```

---

### Task 3: Integration Tests for CLI Auto-Cropping

**Files:**
- Create: `tests/cli_crop_test.rs`

**Step 1: Write integration tests**
1. `test_annotate_with_crop_reduces_dimensions_and_preserves_target`:
   - Create 400x300 image with a "保存" button at `[100.0, 100.0, 80.0, 30.0]`.
   - Run `markits annotate ... --target "保存" --mark rect --crop --crop-margin 10`.
   - Check resulting PNG width and height:
     - Target is 80x30 at (100, 100).
     - Rect stroke may expand slightly; with margin 10, expected width is ~100px (80 + 20) and height is ~50px (30 + 20).
     - Verify dimensions are < 400x300 and within expected crop box.
2. `test_annotate_crop_translates_uimap_metadata`:
   - Inspect output image with `raster::inspect_image`.
   - Verify embedded UIMap exists, contains the element, and the element's `x` and `y` are translated (e.g. close to margin 10.0, not original 100.0).

**Step 2: Run test to verify it passes**
Run: `cargo test --test cli_crop_test`
Expected: PASS

**Step 3: Commit**
```bash
git add tests/cli_crop_test.rs
git commit -m "test(cli): add integration tests for --crop and --crop-margin"
```

---

### Task 4: Add "マークに合わせてクロップ" in Desktop GUI

**Files:**
- Modify: `apps/desktop/src/editor.ts`
- Modify: `apps/desktop/index.html`

**Step 1: Add `cropToAnnotations(margin: number = 32)` method in `apps/desktop/src/editor.ts`**
- Iterate over `this.scene.annotations`.
- Compute union bounds of targets and arrow points.
- Calculate clamped crop rectangle `[rx, ry, rw, rh]`.
- Assign `this.cropRect = { x: rx, y: ry, width: rw, height: rh }` and call `this.applyCrop()`.

**Step 2: Add UI button in `apps/desktop/index.html`**
- In the crop toolbar or header toolbar, add:
  `<button id="btn-crop-to-marks" class="btn btn-secondary btn-sm" title="マークの外接矩形でクロップ">マークに合わせる</button>`
- Wire click listener in `editor.ts` constructor or setup methods:
  `document.getElementById('btn-crop-to-marks')?.addEventListener('click', () => this.cropToAnnotations());`

**Step 3: Verify build in `apps/desktop`**
Run:
```bash
cd apps/desktop && npm run build
```
Expected: PASS with 0 build errors.

**Step 4: Commit**
```bash
git add apps/desktop/src/editor.ts apps/desktop/index.html
git commit -m "feat(desktop): add crop to annotations action in desktop editor"
```

---

### Task 5: Documentation Update and Workspace Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/AI_MANUAL.md`

**Step 1: Update Documentation**
- Add explanation and examples of `--crop` and `--crop-margin <PX>` in `README.md` (under CLI usage and AI UI workflow sections).
- Add `--crop` flags and tips in `docs/AI_MANUAL.md`.

**Step 2: Full Workspace Verification**
Run:
```bash
cargo test --workspace
cd apps/desktop && npm test && npm run build
```
Expected: All tests pass (85+ tests).

**Step 3: Commit**
```bash
git add README.md docs/AI_MANUAL.md
git commit -m "docs: document --crop and --crop-margin in README and AI_MANUAL"
```
