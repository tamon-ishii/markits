# SVG Renderer Specification

## Purpose

Renders resolved annotations into transparent vector SVG markup with cohesive semantic theme styling, spotlight masking, arrow skins, and raster composition with pixel redaction.

## Requirements

### Requirement: Transparent Canvas SVG Root Generation
The system SHALL generate an `<svg>` root element with `xmlns="http://www.w3.org/2000/svg"`, `viewBox="0 0 <width> <height>"`, and explicit `width` and `height` matching the canvas dimensions without an opaque background fill.

#### Scenario: Generate transparent root element
- **WHEN** rendering a scene with canvas dimensions 1920x1080
- **THEN** output SVG starts with `<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">` and no background fill

### Requirement: Semantic Theme Token Application
The system SHALL map all seven semantic styles (`primary`, `secondary`, `warning`, `danger`, `info`, `step`, `pink`) to consistent design tokens controlling stroke colors, fills, light fills, text colors, and stroke widths.

#### Scenario: Render semantic theme colors
- **WHEN** annotations specify `primary`, `warning`, or `pink`
- **THEN** the rendered SVG applies each theme's corresponding stroke, fill, and text color tokens

### Requirement: Geometric Shape and Divider Rendering
The system SHALL render `rect`, `rounded-rect` (with `rx`/`ry`), `circle`/`ellipse`, and `divider` elements using their semantic theme colors, configurable `stroke_width`, `line_style` (`solid`, `dashed`, `dotted`), and shape `fill_style` (`none`, `light`, `solid`).

#### Scenario: Render styled rounded rectangle
- **WHEN** a `rounded-rect` specifies `line_style: "dashed"` and `fill_style: "none"`
- **THEN** output contains a `<rect>` with `rx`, `ry`, `fill="none"`, and `stroke-dasharray`

### Requirement: Arrow and Bezier Arrow Skin Rendering
The system SHALL render `arrow` and `bezier-arrow` marks according to `arrow_skin`: `classic` with a line or quadratic path supporting `line_style` and `filled`/`open` arrowheads, `sketch` as an outlined silhouette filled with a diagonal hatch `<pattern>`, and `bold` as a tapered filled silhouette.

#### Scenario: Render sketch and bold arrow skins
- **WHEN** an arrow or bezier arrow specifies `arrow_skin: "sketch"` or `"bold"`
- **THEN** output renders the full-silhouette `<path>` with hatch fill and outline for `sketch` or solid tapered fill for `bold`

#### Scenario: Render classic dashed open arrow
- **WHEN** a classic arrow specifies `line_style: "dashed"` and `arrowhead: "open"`
- **THEN** output renders a dashed shaft path and an unfilled stroked arrowhead path

### Requirement: Text, Label, Callout, and Badge Rendering
The system SHALL render `label`, `callout` (with pointer arrow and optional target box), `badge` (with optional target box), and `step-arrow` elements with multiline `<tspan>` support and per-annotation `font_family` / `font_size` overrides. When `outline` is enabled, boxed pills SHALL use a white border (`#ffffff`) and unboxed arrow text SHALL use a white halo (`stroke="#ffffff" stroke-width="1.8" paint-order="stroke fill"`).

#### Scenario: Render callout with multiline text and pointer
- **WHEN** a `callout` annotation wraps across multiple lines
- **THEN** output contains a connecting `<line>` with `marker-end`, a pill `<rect>`, and `<tspan>` lines inside `<text>`

#### Scenario: Render unboxed outlined arrow text
- **WHEN** an arrow specifies `boxed: false` and `outline: true`
- **THEN** output `<text>` includes `stroke="#ffffff"`, `stroke-width="1.8"`, and `paint-order="stroke fill"`

### Requirement: Spotlight SVG Masking
For `spotlight` annotations, the system SHALL generate an SVG `<mask>` cutting out the target rectangle and apply a semi-transparent backdrop `<rect>` extending beyond the canvas viewport so antialiasing does not leave an undimmed strip at canvas edges.

#### Scenario: Render spotlight mask with edge bleed
- **WHEN** a scene contains a `spotlight` annotation over a target rectangle
- **THEN** output SVG defines an oversized `<mask>` cutting out the target and applies an oversized dimmed `<rect>` referencing that mask

### Requirement: Skitch Visual Elements and Drop Shadows
The system SHALL render `pin` marks (directional triangle, circular head, optional icon, and dark text pill) and `bullseye` marks (concentric rings with a center dot), and SHALL emit and apply `filter="url(#markits-shadow)"` only when shadow is enabled for an element.

#### Scenario: Render pin and bullseye marks
- **WHEN** a scene contains a `pin` with icon/text and a `bullseye`
- **THEN** output SVG renders the pin's polygon tip, head circle, and pill, and the bullseye's concentric circles and center dot

#### Scenario: Omit drop shadow filter when disabled
- **WHEN** a scene sets `"shadow": false` and no annotation overrides shadow to `true`
- **THEN** output SVG omits the `#markits-shadow` filter definition and `filter` attributes

### Requirement: Raster Composition and Mosaic Redaction
When compositing onto a raster image, the system SHALL apply visible `mosaic` annotations by averaging source image pixels in `block_size` tiles before overlaying the rendered SVG, while ignoring annotations listed in `hidden_annotations`.

#### Scenario: Apply visible mosaic during raster export
- **WHEN** a raster export renders a scene containing a visible `mosaic` mark and a hidden `mosaic` mark
- **THEN** only the visible `mosaic` region is pixelated in the output image pixels
