## Purpose

Renders resolved annotations and layouts into high-quality, transparent vector SVG markup with cohesive theme styling and spotlight masking.

## ADDED Requirements

### Requirement: Transparent Canvas SVG Root Generation
The system SHALL generate an `<svg>` root element with `xmlns="http://www.w3.org/2000/svg"`, `viewBox="0 0 <width> <height>"`, and explicit `width` and `height` matching the canvas dimensions. The root background SHALL be transparent.

#### Scenario: Generate transparent root element
- **WHEN** rendering a scene with canvas dimensions 1920x1080
- **THEN** output SVG starts with `<svg ... width="1920" height="1080" viewBox="0 0 1920 1080">` without an opaque background fill

### Requirement: Semantic Theme Token Application
The system SHALL map semantic styles (`primary`, `secondary`, `warning`, `danger`, `info`, `step`) to consistent design tokens controlling stroke colors, fills, opacity, stroke widths, font family, and font size without requiring caller-provided color codes.

#### Scenario: Render primary callout styling
- **WHEN** an annotation has style `primary`
- **THEN** the rendered SVG applies primary theme stroke, fill, and text colors

#### Scenario: Render warning badge styling
- **WHEN** an annotation has style `warning`
- **THEN** the rendered SVG applies high-visibility warning theme colors

### Requirement: Shape and Annotation Markup Generation
The system SHALL render clean SVG elements for supported annotation types:
- `rect` / `rounded-rect`: `<rect>` element with stroke and optional rx/ry corner rounding.
- `circle` / `ellipse`: `<circle>` or `<ellipse>` element.
- `arrow`: `<path>` or `<line>` with marker-end arrowhead definition.
- `label`: `<text>` element with styled background pill `<rect>`.
- `callout`: grouped `<g>` containing styled text box and connecting pointer arrow.
- `badge`: circular `<circle>` background containing centered step number `<text>`.

#### Scenario: Render rounded rectangle highlighting target
- **WHEN** a `rounded-rect` annotation is rendered
- **THEN** output contains a `<rect>` element with `rx` and `ry` attributes set according to theme standards

#### Scenario: Render callout with text and arrow
- **WHEN** a callout annotation is rendered
- **THEN** output contains a label background rect, label text, and a directional arrow path pointing toward target

### Requirement: Spotlight SVG Masking
For `spotlight` annotations, the system SHALL generate an SVG `<mask>` and semi-transparent overlay covering the canvas while cutting out or highlighting the target area.

#### Scenario: Render spotlight mask
- **WHEN** a scene contains a `spotlight` annotation over a target rectangle
- **THEN** output SVG defines a `<mask>` cutting out target coordinates and applies an overlay `<rect>` referencing that mask
