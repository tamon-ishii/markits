# MarkIts AI Manual

For agents that can view images and run local commands. MarkIts draws annotations at pixel coordinates or resolved UI targets supplied in JSON or extracted from image UIMap metadata.

## Image-to-image workflow

1. View the source image or inspect UI elements using `markits uimap screenshot.png`.
   Export the map for later commands with `markits uimap screenshot.png --json --output uimap.json`.
2. Run `markits inspect screenshot.png` for dimensions and embedded UIMap status. Output: `{"width":800,"height":480,"format":"png","has_uimap":true,"uimap_elements_count":5}`.
3. If annotating by UI element name, you can use:
   - **Quick Command (No JSON file needed)**:
     `markits annotate screenshot.png -o annotated.png --target "保存" --mark pin --text "ここをクリック"`
   - **Custom Scene JSON**: write JSON using `"target": "保存"` or `[x, y, w, h]`.
4. Run `markits validate annotations.json --image screenshot.png` (supports `--uimap elements.json` if using external UI map).
5. Run `markits render annotations.json --image screenshot.png --output annotated.png`.
6. Output PNG automatically preserves UIMap metadata so further annotations can be chained.

PNG and JPEG input are supported; image output is PNG. `--output` must end in `.png`. If JSON explicitly includes `canvas`, its dimensions must match the image. Errors go to stderr with a nonzero exit status.

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
- `--position <POS>`: Placement hint: `auto`, `top`, `bottom`, `left`, `right`.
- `--uimap <PATH>`: Optional external UIMap JSON file (if image doesn't have embedded metadata).

### Natural-language UIMap example

For a request such as “put a 1 mark on the Save button”, inspect the UIMap first, then use the matching numbered element:

```bash
markits uimap screenshot.png --json --output uimap.json
markits annotate screenshot.png --uimap uimap.json \
  --target 1 --mark badge --step 1 --style step --output annotated.png
```

## Crop a screenshot

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

Other supported types: `pin`, `arrow`, `bezier-arrow`, `bullseye`, `divider`. `arrow` and `bezier-arrow` support text labels (`"text":"Label"`) with `"text_placement": "middle"` (default) or `"end"` (tip of the arrow).

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
