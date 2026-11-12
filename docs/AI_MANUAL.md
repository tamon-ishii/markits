# MarkIts AI Manual

For agents that can view images and run local commands. MarkIts draws annotations at pixel coordinates or resolved UI targets supplied in JSON or extracted from image UIMap metadata.

## Screen Capture & Image Workflow

### Remove MarkIts edit metadata (`markits strip-metadata`)

```sh
markits strip-metadata private.png -o share.png
markits strip-metadata private.webp -o share.webp
markits strip-metadata private.jxl -o share.jxl
```

Removes MarkIts original/base images, annotations, crop state, UIMap, capture
identity, operation recordings, and operation groups. Compressed pixel data is
copied without decoding/re-encoding; visible annotations and mosaic stay in the
image. Other applications' metadata and color profiles are retained. Supported
containers are PNG, WebP and JPEG XL. Bare JXL codestreams have no metadata and
are copied unchanged. Use a different output file; in-place writes are rejected.
The output remains in the input format, regardless of its filename extension.

Desktop **Save** exports flattened PNG, lossless WebP, or lossless JPEG XL
without edit metadata. Editable source images, UIMap, crop state and operations
remain in desktop history. CLI capture/render commands retain their existing
UIMap behavior. Add `--editing-metadata` to render/annotate/crop to preserve editable source, annotations and crop state; operation recordings are carried over when present. Use `export` (without this flag) or `strip-metadata` for sharing.

### 1. Standalone Screen Capture (`markits capture`)
Take screenshots directly using MarkIts CLI:
```bash
# Fast capture without UI detection (<20ms)
markits capture screenshot.png

# Capture with desktop UI detection (embeds UIMap into PNG metadata)
markits capture screenshot.png --detect-ui

# List displays, then capture the second display (indices start at 0)
markits capture --list-screens
markits capture second-display.png --screen 1

# List windows, then capture one by a partial title, window ID, or process ID
markits capture --list-windows
markits capture app-window.png --window "設定"
markits capture app-window.png --pid 1234

# REUSE UIMAP: Fast capture carrying over UIMap metadata from a previous screenshot
markits capture screenshot2.png --uimap screenshot.png

# Capture and immediately box a button in one shot
markits capture annotated.png --uimap screenshot.png --target "保存ボタン" --mark rect

# Capture, mark target, and auto-crop tightly around it in one shot
markits capture button_crop.png --target "保存ボタン" --mark rect --crop --crop-margin 16
```

### 2. Inspecting and Reusing UIMap
1. View source image or inspect UI elements using `markits uimap screenshot.png`.
   Export map as JSON if needed: `markits uimap screenshot.png --json --output uimap.json`.
2. Run `markits inspect screenshot.png` for dimensions and embedded UIMap status.
   Output: `{"width":800,"height":480,"format":"png","has_uimap":true,"uimap_elements_count":5}`.
3. Annotating by UI element name:
   - **Quick Command**:
     `markits annotate screenshot.png -o annotated.png --target "保存ボタン" --mark rect`
     (or `--mark pin --text "ここをクリック"`)
   - **External / Carried-Over UIMap**: `--uimap` accepts both `.json` files AND `.png` images containing embedded metadata!
4. Output PNG automatically preserves UIMap metadata so further annotations can be chained.

PNG, JPEG, WebP and JPEG XL input are supported. Image outputs from capture, annotate, annotate-batch, render, crop and export accept `.png`, `.webp`, or `.jxl`. WebP and JPEG XL are lossless. If JSON explicitly includes `canvas`, its dimensions must match the image. Errors go to stderr with a nonzero exit status.


## Extended CLI workflows

All functionality below is discoverable through `markits --help`,
`markits <command> --help`, and this bundled manual; no repository files are required.

### Launch, operate and capture with an embedded recipe

```sh
# Embed the script CONTENTS, literal arguments, absolute cwd and recorded operations
markits capture-run --launch-script ./start.ps1 --arg "project name" --cwd "C:/work/app" --window "My App" --operations session-01/recording.json --window-bounds "[100,100,1280,800]" -o app.png

# Or launch an executable directly; repeat --arg for each argument
markits capture-run --launch "C:/tools/MyApp.exe" --arg=--project --arg "C:/work/project" --cwd "C:/work/app" -o app.webp

# Re-execute the embedded launch and window operations, then take a new image
markits recapture app.png -o new.png

# Inspect/extract the recipe WITHOUT executing anything
markits recipe app.png
markits recipe app.png --output recipe.json
markits recapture app.png --dry-run

# The extracted JSON can be edited and reused directly
markits capture-run --recipe recipe.json -o from-json.jxl
```

`capture-run` starts the program/script, waits for a unique window, optionally
sets window bounds, waits `--settle-ms` (default 1000), replays operations, detects
UI elements, and saves the screenshot(s). Without `--operations`, or when the
recording has no screenshot points, it automatically adds a final capture.
`--operations` accepts the `recording.json` produced by `record finish` or a raw
`OperationRecording` JSON object. Bounds placement and cursor inclusion are
currently Windows features. On Windows, current outer window bounds are saved
automatically when bounds were not specified.

`--window` is a unique title substring. It is required for embedded scripts;
for a direct executable, its process ID can select the window automatically.
Launched processes are matched by PID or newly appearing matching windows, so
an older window with the same title is not accidentally operated. Launchers that
reuse an existing single-instance window should instead prepare that application
and use `capture-run --window "My App"` without launch options.
`--timeout-ms` bounds the window wait (default 30000).

The version-1 `markits:capture_recipe` metadata contains `launch.program` OR
`launch.script` (`name`, full UTF-8 `content`, optional `runner`), `launch.args`
(an array of literal strings), absolute `launch.cwd`, `window`, `window_bounds`,
`recording` (all clicks, text, keys, scrolls and capture points), timing, cursor
and UI-depth settings, and a zero-based `capture_index` for the represented frame.
An extracted JSON without `capture_index` generates all screenshot points.
For multiple outputs, the first uses the requested filename and subsequent
images use `_002`, `_003`, etc. Each image selects its own frame on recapture.

Scripts are materialized from embedded contents in a new `markits-run-*` directory
beside the output image; deleting the original script file does not prevent
recapture. `.ps1` uses PowerShell, `.py` uses `python`, and `.sh` uses `sh`.
Use `--script-runner` for another interpreter executable. The specified working
directory and any script dependencies must still exist. Startup arguments are
passed individually, without constructing a shell command. Launch stdout/stderr
logs and intermediate captures remain in the run directory for inspection.
The launched application is left running.

`recapture` explicitly executes the saved launch script and operation recording.
If an image was annotated/cropped with `--editing-metadata`, it reapplies those
marks and crop to the fresh capture. `recipe` and `--dry-run` only inspect JSON
and never launch or send input. `inspect` reports `has_capture_recipe`.
Recipes survive editable PNG/WebP/JPEG XL conversion, desktop history and edits
that retain editing metadata. `export` without `--editing-metadata` and
`strip-metadata` remove the recipe, including scripts, arguments and cwd.

### Java UI detection and cursor capture

`capture --list-windows --json` lists native window IDs and PIDs. Select a unique
ID or title with `--window`, or a PID with `--pid`. Ambiguous selections fail.
On Windows, UI detection tries Java Access Bridge before UI Automation. The target
Java runtime must provide and enable its accessibility bridge. Exploration depth
is configurable from 1 to 128 (default 64).

```sh
markits capture --list-windows --json
markits capture app.png --window "My Java App" --detect-ui --ui-depth 128
markits capture cursor.png --window "My App" --include-cursor --detect-ui
```

`--include-cursor` is Windows only and also works for screen/region capture.
It captures the pointer at its current position; move it before invoking capture.

### Editable images and lossless formats

```sh
markits annotate app.png --target "Open" --mark rect --crop --crop-margin 16 --editing-metadata -o detail.webp
markits export detail.webp --editing-metadata -o detail.jxl
markits export detail.jxl -o share.png
```

`--editing-metadata` stores an unpainted background, editable annotations,
UIMap, crop offsets and the uncropped base image. Embedded operation recordings
and capture identity are retained. The desktop editor can reopen these files.
Without this flag, annotate/render retain their existing UIMap behavior but do
not save an editable annotation scene. `export` flattens and removes all MarkIts
metadata by default; add the flag to retain existing metadata during conversion.
It does not invent editing data for a flattened image.

### Desktop capture history

```sh
markits history list
markits history save detail.webp
markits history load CAPTURE_ID -o restored.png
markits history delete CAPTURE_ID
```

History uses the desktop app's data directory and format. List/save return JSON.
Load includes the editable payload and crop base when available. Delete removes
the named capture and its thumbnail/base image. Set `MARKITS_HISTORY_DIR` to use
an alternate history directory, for example in tests.

### Record operations and screenshot points

```sh
markits record start --session session-01 --window "My App"
markits record status --session session-01
markits record screenshot --session session-01 --include-cursor
markits record pause --session session-01
markits record resume --session session-01
markits record finish --session session-01
# To discard the operation log instead:
# markits record cancel --session session-01
```

Start creates a NEW session directory and launches a background recorder, then
returns. The recorder observes input targeting the selected window. Use separate
CLI invocations to pause/resume, take screenshot points, inspect status, or stop.
The screenshot command also detects UI elements. Finish writes `recording.json`
and embeds the complete operation recording into `capture_001.png`, etc. Cancel
stops the owned recorder and leaves previously captured images without embedding
the operation log. Always finish or cancel a session; input monitoring requires
OS accessibility/input permissions. Linux recording requires X11.

### Replay and recapture

```sh
markits replay session-01/recording.json --dry-run
markits replay session-01/recording.json --window "My App" --output-dir recaptured
markits replay session-01/capture_001.png --window "My App" --output-dir recaptured --shorten-waits
```

Replay actually sends clicks, text, keys and scrolls to the selected application.
Restore the application to the recording's initial state before running it.
`--dry-run` validates and prints operations without accessing the desktop.
Named click targets are resolved against the current UI; unnamed coordinates
scale to the target window. Missing/ambiguous named targets and focus loss stop
replay. Screenshots are taken at recorded screenshot points and saved with UIMap
and operation metadata, under unique filenames. With no explicit target, the
recorded window title is used. `--shorten-waits` skips recorded idle delays;
`--max-wait-ms` caps each delay (default 60000). Short settle waits after input
remain. `--include-cursor` forces pointer inclusion on Windows; otherwise each
recorded screenshot's cursor setting is used.

## UIMap & Named Targets

When screenshots are captured with UI metadata (or provided via `--uimap ui.json`), MarkIts automatically matches named targets.

### Listing UI Elements
```bash
# Human-readable list
markits uimap screenshot.png

# Filter by role
markits uimap screenshot.png --filter button

# Machine-readable JSON output
markits uimap screenshot.png --json
```

### Smart Target Name Resolution
In `target` fields (or `--target` argument):
- **Exact Name**: `"target": "保存"` matches element with `name: "保存"`.
- **Fuzzy / Suffix Match**: `"target": "保存ボタン"` automatically strips common Japanese suffixes like "ボタン" to match element `"保存"`.
- **Role Prefix**: `"target": "button:保存"` disambiguates buttons from text labels.
- **Index Reference**: `"target": "ui-1"` or `"target": "button:1"` targets the first UI element or first button.
- **Numeric Index**: `"target": "1"` targets the first UIMap element. Indices are one-based and match the numbered human-readable UIMap output.
- **Explicit Rect**: `[x, y, width, height]` is always supported as fallback.

## Quick Annotate Command (`markits annotate`)

For AI and automation, add annotations in a single command without writing a JSON file:

```bash
markits annotate screenshot.png \
  -o annotated.png \
  --target "保存" \
  --mark pin \
  --text "ここをクリック" \
  --style primary
```

Arguments:
- `--target <TARGET>`: UI element name (`"保存"`), role prefix (`"button:保存"`), or `x,y,w,h` (`100,50,80,32`).
- `--mark <TYPE>`: `pin` (default), `rect`, `rounded-rect`, `callout`, `arrow`, `step-arrow`, `badge`, `spotlight`, `bullseye`.
- `--step <NUMBER>`: Number shown by `badge` or `step-arrow` (for example, `--step 1`).
- `--text <STRING>`: Optional label text.
- `--style <STYLE>`: `primary` (default), `secondary`, `warning`, `danger`, `info`, `step`, `pink`.
- `--uimap <PATH>`: Optional external UIMap file: accepts either a `.json` file or an existing `.png` image with embedded metadata.
- `--crop`: Auto-crop output image to the bounding box enclosing all annotations.
- `--crop-margin <PIXELS>`: Margin around annotation bounding box when cropping (default: `32`).

### Natural-language UIMap example

For a request such as “put a 1 mark on the Save button”, inspect the UIMap first, then use the matching numbered element:

```bash
markits uimap screenshot.png --json --output uimap.json
markits annotate screenshot.png --uimap uimap.json \
  --target 1 --mark badge --step 1 --style step --output annotated.png
```

## Auto-Crop to Annotations (`--crop`)

Add `--crop` and `--crop-margin <PX>` (default `32`) to `markits annotate`, `markits render`, or `markits capture`.
- MarkIts calculates the union bounding box of all annotations and expands it by `crop-margin` pixels (clamped to the image canvas).
- Embedded UIMap metadata is preserved: elements inside the crop are shifted relative to `(0, 0)` of the cropped image.

## Manual Crop

Run `markits crop screenshot.png --x 120 --y 80 --width 640 --height 400 --output cropped.png` to keep exactly that rectangle. The output is a 640 × 400 PNG; pixels outside the rectangle are removed rather than covered. Coordinates are measured from the source image's top-left corner. The rectangle must be nonempty and entirely inside the source image.

## Minimal JSON

```json
{
  "annotations": [
    {"type": "pin", "target": "保存ボタン", "text": "ここをクリック", "style": "primary"},
    {"type": "rounded-rect", "target": "キャンセル", "style": "secondary"}
  ]
}
```

If coordinates are used directly, `target` is `[x, y, width, height]` in image pixels.

## Choose an annotation

| Request | Annotation JSON entry |
| --- | --- |
| Box a region | `{"type":"rect","target":"保存"}` or `[x,y,w,h]` |
| Box with rounded corners | `{"type":"rounded-rect","target":"保存","rx":8}` |
| Pin tag with icon/text | `{"type":"pin","target":"保存","text":"Click here"}` |
| Add a callout with pointer | `{"type":"callout","target":"保存","text":"Explanation","outline":false}` |
| Add a label only | `{"type":"label","target":"保存","text":"Explanation","outline":false}` |
| Number a step | `{"type":"badge","target":"保存","step":1}` |
| Number with arrow | `{"type":"step-arrow","target":"保存","step":1}` |
| Dim outside a region | `{"type":"spotlight","target":"保存"}` |
| High-level click instruction | `{"type":"instruction","action":"click","target":"保存","text":"Click here"}` |

Other supported types: `pin`, `arrow`, `bezier-arrow`, `bullseye`, `divider`. `arrow` and `bezier-arrow` support text labels (`"text":"Label"`) with `"text_placement": "middle"` (default) or `"end"`. `middle` uses the shaft center as its anchor and moves the label clear of the line according to its bounds and arrow direction. `end` places it beyond the tail, on the side without the triangular head. For Bezier arrows, `position` controls the side used for a centered label.

## Reuse targets

```json
{
  "targets": {"save-button": [240, 180, 180, 50]},
  "annotations": [
    {"type": "rect", "target": "save-button"},
    {"type": "callout", "target": "save-button", "text": "Click Save", "outline": false}
  ]
}
```

## Other output modes

For SVG output, include `"canvas":{"width":800,"height":480}` in JSON. `markits render annotations.json > overlay.svg` writes a transparent SVG. `markits render annotations.json --layout-json > layout.json` writes the SVG plus element IDs, bounds, and arrow paths as JSON. `markits manual` prints this document from the installed binary.
