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

## Minimal JSON

```json
{
  "annotations": [
    {"type": "rect", "target": [240, 180, 180, 50], "style": "primary"},
    {"type": "callout", "target": [240, 180, 180, 50], "text": "Click Save", "position": "bottom"}
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
| Add a label and pointer | `{"type":"callout","target":[x,y,w,h],"text":"Explanation"}` |
| Add a label only | `{"type":"label","target":[x,y,w,h],"text":"Explanation"}` |
| Number a step | `{"type":"badge","target":[x,y,w,h],"step":1}` |
| Number with arrow | `{"type":"step-arrow","target":[x,y,w,h],"step":1}` |
| Dim outside a region | `{"type":"spotlight","target":[x,y,w,h]}` |
| Highlight a click with text | `{"type":"instruction","action":"click","target":[x,y,w,h],"text":"Click here"}` |

Other supported types: `pin`, `arrow`, `bezier-arrow`, `bullseye`, `divider`. Common `style` values: `primary`, `secondary`, `warning`, `danger`, `info`, `step`, `pink`. `position` can hint `auto`, `top`, `bottom`, `left`, `right`, or a corner such as `top-left`. Optional `shadow` and `outline` flags affect text marks. Run `validate` after constructing JSON; it reports unknown fields and invalid values.

## Reuse targets

```json
{
  "targets": {"save-button": [240, 180, 180, 50]},
  "annotations": [
    {"type": "rect", "target": "save-button"},
    {"type": "callout", "target": "save-button", "text": "Click Save"}
  ]
}
```

## Other output modes

For SVG output, include `"canvas":{"width":800,"height":480}` in JSON. `markits render annotations.json > overlay.svg` writes a transparent SVG. `markits render annotations.json --format layout-json > layout.json` writes the SVG plus element IDs, bounds, and arrow paths as JSON. `--format layout-json` cannot be combined with `--image`. `markits manual` prints this document from the installed binary.
