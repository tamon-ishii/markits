# MarkIts Auto-Crop to Annotation Bounding Box Design

## Overview
This design introduces an automatic cropping capability to MarkIts. When placing annotations on an image or screenshot, users and AI agents can crop the output image down to the bounding box enclosing all annotations, with a configurable margin in pixels.

This enables streamlined workflows where an AI or user targets a specific UI component (e.g. "Save button", "Search bar"), marks it, and receives a tight, focused crop around that component in a single command.

---

## 1. CLI Interface

The `--crop` flag and `--crop-margin <PX>` option are added to the following CLI subcommands:
- `markits annotate <INPUT> -o <OUTPUT> [--crop] [--crop-margin <PX>] ...`
- `markits render <IMAGE> <LAYOUT_JSON> -o <OUTPUT> [--crop] [--crop-margin <PX>] ...`
- `markits capture [-o <OUTPUT>] [--crop] [--crop-margin <PX>] ...`

### Options
- `--crop`: Enable auto-cropping to the bounding box of rendered annotations.
- `--crop-margin <PX>`: Margin in pixels to expand around the bounding box (default: `32`).

### Examples
```bash
# Capture screen, detect and mark "Save" button, and crop image to button with 24px padding
markits capture -o button_crop.png --target "保存ボタン" --mark rect --crop --crop-margin 24

# Annotate existing image and crop to marks with default 32px padding
markits annotate input.png -o target_crop.png --target "検索" --mark rect --crop

# Render from JSON layout and crop to marks
markits render input.png layout.json -o cropped.png --crop --crop-margin 16
```

---

## 2. Core Architecture & Data Flow

### Bounding Box Calculation
1. When rendering SVG annotations from JSON/layout (`render_with_layout_from_json`), the layout engine outputs `RenderResult { svg, elements: Vec<LayoutElement> }`.
2. Each `LayoutElement` specifies `bounds: [f64; 4]` representing `[x, y, width, height]`.
3. The union bounding box is computed over all rendered elements:
   - `min_x = min(element.bounds[0])`
   - `min_y = min(element.bounds[1])`
   - `max_x = max(element.bounds[0] + element.bounds[2])`
   - `max_y = max(element.bounds[1] + element.bounds[3])`
4. Expand by `margin`:
   - `crop_x = max(0, floor(min_x - margin))`
   - `crop_y = max(0, floor(min_y - margin))`
   - `crop_w = min(canvas_width - crop_x, ceil(max_x + margin) - crop_x)`
   - `crop_h = min(canvas_height - crop_y, ceil(max_y + margin) - crop_y)`

### Raster Cropping
- Apply `imageops::crop_imm` to the composed RGBA image using `(crop_x, crop_y, crop_w, crop_h)`.

### UIMap Metadata Translation
If the input image contains embedded UIMap metadata:
- Use existing `ui_elements::filter_elements_for_crop(elements, crop_x, crop_y, crop_w, crop_h)` to keep elements within the crop area and offset their coordinates relative to the new origin `(crop_x, crop_y)`.
- Re-embed the updated UIMap into the cropped PNG metadata via `embed_png_uimap`.

---

## 3. Desktop GUI Integration

In `apps/desktop/src/editor.ts` and `apps/desktop/index.html`:
- Add a "マークに合わせてクロップ (Crop to Marks)" button/menu action.
- Action:
  1. Computes the union bounding box of all current annotations.
  2. Expands by margin (default 32px).
  3. Crops the canvas/background image.
  4. Translates all existing annotation coordinates and UIMap elements by `(-crop_x, -crop_y)`.
  5. Updates canvas dimensions and triggers re-render.

---

## 4. Error Handling & Edge Cases

- **No annotations present**:
  If `--crop` is enabled but no annotations are generated/specified, output the uncropped image and print a gentle warning to stderr without failing.
- **Margin extends beyond canvas bounds**:
  Clamped strictly to `[0, canvas_dimension]` to prevent panics or negative dimensions.
- **Zero width/height elements**:
  Guarded by `min(1, ...)` so crops always produce at least 1x1 valid images.

---

## 5. Verification Plan

1. **Unit / Integration Tests**:
   - `tests/cli_crop.rs`:
     - Test `markits annotate --crop` crops to annotation bounds + margin.
     - Test custom `--crop-margin` values (e.g. 0, 16, 50).
     - Test UIMap metadata coordinates are translated properly after cropping.
     - Test behavior when no annotations exist.
2. **Workspace Test Suite**:
   - Run `cargo test --workspace` and ensure all tests pass.
   - Run `npm test` / `npm run build` in `apps/desktop`.
