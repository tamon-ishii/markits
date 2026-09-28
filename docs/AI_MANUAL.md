# MarkIts AI Manual

For agents that can view images and run local commands. MarkIts draws annotations at pixel coordinates supplied in JSON. It does not detect UI elements or interpret natural language itself.

## Image-to-image workflow

1. View the source image and identify the requested target's bounding rectangle.
2. Run `markits inspect screenshot.png` for its dimensions if needed. Output: `{"width":800,"height":480,"format":"png"}`.
3. Write annotation JSON. For image output, omit `canvas`; MarkIts uses the source image dimensions.
4. Run `markits validate annotations.json --image screenshot.png`.
5. Run `markits render annotations.json --image screenshot.png --output annotated.png`.
6. View `annotated.png` and adjust coordinates if the mark misses the target.

PNG and JPEG input are supported; image output is PNG. `--output` must end in `.png`. If JSON explicitly includes `canvas`, its dimensions must match the image. Errors go to stderr with a nonzero exit status.

## Crop a screenshot

Run `markits crop screenshot.png --x 120 --y 80 --width 640 --height 400 --output cropped.png` to keep exactly that rectangle. The output is a 640 × 400 PNG; pixels outside the rectangle are removed rather than covered. Coordinates are measured from the source image's top-left corner. The rectangle must be nonempty and entirely inside the source image.

To annotate the cropped image, use `cropped.png` as `--image` and measure all annotation targets relative to its new top-left corner. A target at source position `(240, 180)` becomes `(120, 100)` in this example. Inspect the cropped result before annotating.

## Minimal JSON

```json
{
  "annotations": [
    {"type": "rect", "target": [240, 180, 180, 50], "style": "primary"},
    {"type": "callout", "target": [240, 180, 180, 50], "text": "Click Save", "position": "bottom", "outline": false}
  ]
}
```

`target` means `[x, y, width, height]` in image pixels, origin at the top left. An object with `x`, `y`, `width`, and `height` also works. Width and height must be positive. Use precise visual bounds; MarkIts cannot locate a button from its label.

## Choose an annotation

| Request | Annotation JSON entry |
| --- | --- |
| Box a region | `{"type":"rect","target":[x,y,w,h]}` |
| Box with rounded corners | `{"type":"rounded-rect","target":[x,y,w,h],"rx":8}` |
| Circle a region | `{"type":"circle","target":[x,y,w,h]}` |
| Add a label and pointer | `{"type":"callout","target":[x,y,w,h],"text":"Explanation","outline":false}` |
| Add a label only | `{"type":"label","target":[x,y,w,h],"text":"Explanation","outline":false}` |
| Number a step | `{"type":"badge","target":[x,y,w,h],"step":1}` |
| Number with arrow | `{"type":"step-arrow","target":[x,y,w,h],"step":1}` |
| Dim outside a region | `{"type":"spotlight","target":[x,y,w,h]}` |
| Highlight a click with text | `{"type":"instruction","action":"click","target":[x,y,w,h],"text":"Click here","outline":false}` |

Other supported types: `pin`, `arrow`, `bezier-arrow`, `bullseye`, `divider`. Common `style` values: `primary`, `secondary`, `warning`, `danger`, `info`, `step`, `pink`. `position` can hint `auto`, `top`, `bottom`, `left`, `right`, or a corner such as `top-left`. Text marks have a white outline by default; set `"outline":false` when rendering white text on a filled label. Optional `shadow` controls the drop shadow. Run `validate` after constructing JSON; it reports unknown fields and invalid values.

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

For SVG output, include `"canvas":{"width":800,"height":480}` in JSON. `markits render annotations.json > overlay.svg` writes a transparent SVG. `markits render annotations.json --layout-json > layout.json` writes the SVG plus element IDs, bounds, and arrow paths as JSON. `markits render annotations.json --debug > layout-debug.svg` shows layout diagnostics. `--layout-json` and `--debug` cannot be combined with `--image`. `markits manual` prints this document from the installed binary.
