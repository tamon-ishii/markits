# Design: Standalone Screen Capture, UIMap Reuse, and UI Targeting

**Date**: 2026-10-03  
**Status**: Approved  

---

## 1. Context & Motivation

MarkIts allows AI agents and human users to add annotations (boxes, arrows, pins, spotlights, badges) to screenshots based on coordinates or named UI elements.

Previously:
- Screen capture and AT-SPI/X11 UI detection were only available inside the Tauri desktop GUI app (`apps/desktop/src-tauri`).
- The standalone CLI (`markits`) only operated on existing image files on disk.
- If an AI wanted to capture the current screen and draw a box around a button (e.g., "保存ボタンを囲む"), it had no CLI command to take a screenshot directly.
- AT-SPI UI accessibility tree scanning is heavy (taking ~1–3s depending on open applications). While MarkIts embeds detected UIMap in PNG metadata (`markits:ui_elements` chunk), there was no way for the CLI to capture a fresh screenshot and reuse an existing UIMap from a previously captured image metadata.

This design introduces:
1. `markits capture <OUTPUT>`: A standalone CLI command to capture screens.
2. `--detect-ui`: Optional UI element detection embedded into PNG metadata.
3. `--uimap <PATH>`: Reusing UIMap metadata from an existing `.png` image or `.json` file during capture and annotation without rescanning AT-SPI.
4. UI Targeting and "囲む" (surround/box): Enabling LLMs to specify `--target "保存ボタン" --mark rect` (or `rounded-rect`), with smart target matching and one-shot capture + annotate.
5. `README.md` and `AI_MANUAL.md` overhaul: Dedicating a major top-level section explaining UI targeting, UIMap metadata embedding, and AI agent workflows.

---

## 2. Architecture & Subcommand Design

### 2.1 Subcommand: `markits capture`

```bash
markits capture [OPTIONS] <OUTPUT>
```

#### Arguments & Flags:
- `<OUTPUT>`: Destination `.png` image path.
- `--detect-ui`: Perform UI element detection on the captured screen and embed the resulting UIMap in the output PNG metadata.
- `--uimap <PATH>`: Load UIMap from an existing `.png` (extracted from `markits:ui_elements` metadata) or `.json` file, and embed it into `<OUTPUT>` without running UI detection.
- Region / Window options:
  - `--x <U32>`, `--y <U32>`, `--width <U32>`, `--height <U32>`: Capture a specific subregion instead of the full primary screen.
- Annotation options (One-shot capture & annotate):
  - `--target <TARGET>`: UI element name (e.g. `"保存"`, `"保存ボタン"`) or `[x, y, w, h]`.
  - `--mark <TYPE>`: Mark type (default: `rect`, supports `rounded-rect`, `pin`, `callout`, `arrow`, `spotlight`, `badge`).
  - `--text <STRING>`: Optional label text.
  - `--step <NUMBER>`: Number for step-arrow/badge.
  - `--style <STYLE>`: Semantic style (`primary`, `secondary`, `warning`, `danger`, `info`, `step`).
  - `--position <POS>`: Placement hint (`auto`, `top`, `bottom`, `left`, `right`).

### 2.2 UIMap Reuse in Other Commands

Extend `--uimap <PATH>` across all CLI subcommands (`annotate`, `render`, `validate`, `uimap`):
- If `<PATH>` ends in `.png`, read the image and extract the embedded UIMap metadata.
- If `<PATH>` ends in `.json`, parse as JSON array of `UiElement`.

### 2.3 UI Targeting & "保存ボタンを囲む"

LLM agents identify UI elements and specify annotations:
- To box/surround: `--mark rect` (or `rounded-rect`).
- Target resolution in `src/semantic.rs`:
  - Automatically matches `"保存ボタン"` -> `"保存"` by stripping the `"ボタン"` suffix.
  - Supports role prefixes (e.g. `button:保存`) and numbered indices (e.g. `1`, `ui-1`).
- AI workflow:
  ```bash
  # Step 1: Capture with UI detection (once)
  markits capture screen.png --detect-ui

  # Step 2: AI inspects UI elements
  markits uimap screen.png

  # Step 3: AI annotates "保存ボタンを囲む"
  markits annotate screen.png --target "保存ボタン" --mark rect -o boxed.png

  # Step 4: Next screen capture reusing UIMap (super fast, <20ms)
  markits capture screen2.png --uimap screen.png
  ```

---

## 3. Crate & Code Organization

1. **Move / Share Capture & UI Elements Logic**:
   - Move or expose `capture` and `ui_elements` from `apps/desktop/src-tauri` to `markits` root library (`src/capture.rs` and `src/ui_elements.rs`).
   - Root crate `Cargo.toml` includes `screenshots = "0.8.10"` and UI detection dependencies (`xa11y`, `x11-dl`, `zbus`).
   - `apps/desktop/src-tauri` references `markits::capture` and `markits::ui_elements` directly, eliminating duplication.
2. **Metadata Extraction Utility**:
   - `raster::load_uimap_from_path(path: &Path)`:
     If file ends in `.png` (or format is PNG), call `extract_png_uimap`. Otherwise, parse as JSON.
3. **Documentation**:
   - Add a major section in `README.md` and update `docs/AI_MANUAL.md`.

---

## 4. Verification Plan

1. **Unit & Integration Tests**:
   - Test `load_uimap_from_path` with both JSON files and PNG images with embedded metadata.
   - Test `markits capture` command flags parsing and logic.
   - Test one-shot capture + annotate pipeline.
2. **Manual / CLI Verification**:
   - Run `cargo test`.
   - Test `cargo run -- capture test_screen.png`.
   - Test `cargo run -- capture test_ui.png --uimap test_screen.png`.
   - Test `cargo run -- annotate test_screen.png --target "..." --mark rect -o out.png`.
