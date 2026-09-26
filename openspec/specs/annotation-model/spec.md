# Annotation Model Specification

## Purpose

Provides data models and JSON serialization/deserialization for canvases, target bounding boxes, semantic styles, positioning hints, and various annotation types.

## Requirements

### Requirement: Canvas and Target Geometry Parsing
The system SHALL parse canvas dimensions (width and height) and target bounding boxes (x, y, width, height) from structured JSON representations. Target rectangles MUST accept either an object format (`{"x": 0, "y": 0, "width": 100, "height": 50}`) or a 4-element coordinate array format (`[x, y, width, height]`).

#### Scenario: Parse canvas and target object format
- **WHEN** valid JSON with canvas width 1920, height 1080, and target object `{x: 820, y: 640, width: 100, height: 32}` is provided
- **THEN** the system parses the canvas dimensions and target bounding box into strongly-typed geometry models

#### Scenario: Parse target 4-element array format
- **WHEN** valid JSON contains target as array `[820, 640, 100, 32]`
- **THEN** the system interprets the values respectively as x=820, y=640, width=100, height=32

### Requirement: Semantic Annotation Types
The system SHALL support deserializing at least eight core annotation types: `arrow`, `rect`, `rounded-rect`, `circle` / `ellipse`, `label`, `callout`, `badge`, and `spotlight`.

#### Scenario: Parse callout annotation
- **WHEN** JSON specifies an annotation of type `callout` with target bounding box, text "Save Settings", and style `primary`
- **THEN** the system constructs a Callout annotation model retaining target, text, and style attributes

#### Scenario: Parse spotlight annotation
- **WHEN** JSON specifies an annotation of type `spotlight` with target bounding box
- **THEN** the system constructs a Spotlight annotation model referencing the target region

#### Scenario: Parse badge annotation with step counter
- **WHEN** JSON specifies an annotation of type `badge` with a step index or text marker and a target location
- **THEN** the system constructs a Badge annotation model

### Requirement: Semantic Styles and Position Hints
The system SHALL support semantic style names (`primary`, `secondary`, `warning`, `danger`, `info`, `step`) and position hints (`auto`, `top`, `bottom`, `left`, `right`, `top-left`, `top-right`, `bottom-left`, `bottom-right`). If omitted, style SHALL default to `primary` and position SHALL default to `auto`.

#### Scenario: Explicit semantic style and position hint
- **WHEN** an annotation defines `"style": "warning"` and `"position": "top-right"`
- **THEN** the parsed annotation model sets style to Warning and position hint to TopRight

#### Scenario: Default style and position hint fallback
- **WHEN** an annotation JSON omits `style` and `position`
- **THEN** the parsed annotation model defaults to `primary` style and `auto` position hint

### Requirement: Input Validation and Error Handling
The system SHALL validate JSON schema constraints, rejecting negative dimensions, malformed syntax, or unknown annotation types with descriptive error details.

#### Scenario: Reject negative canvas dimensions
- **WHEN** JSON specifies negative width or height for canvas
- **THEN** parsing fails with a validation error describing invalid dimensions

#### Scenario: Reject unknown annotation type
- **WHEN** JSON specifies an unsupported annotation type
- **THEN** parsing fails with an error listing unsupported annotation type

### Requirement: Skitch Style Annotation Types
The system SHALL support deserializing additional visual annotation types: `pin` (pointer tag with icon and optional text pill), `bullseye` (concentric focus rings with center dot), and `divider` (accent dividing line).

#### Scenario: Parse pin callout annotation
- **WHEN** JSON specifies an annotation of type `pin` with target `[100, 200, 50, 50]`, icon "♡", text "お気に入り", and style `pink`
- **THEN** the system parses the attributes into a Pin annotation model

#### Scenario: Parse bullseye annotation
- **WHEN** JSON specifies an annotation of type `bullseye` with target `[300, 300, 60, 60]`
- **THEN** the system parses it into a Bullseye annotation model

#### Scenario: Parse divider annotation
- **WHEN** JSON specifies an annotation of type `divider` with target coordinate bounds
- **THEN** the system parses it into a Divider annotation model

### Requirement: Drop Shadow and Text Outline Options
The system SHALL support configuring drop shadow visibility (`shadow: bool` on Scene, `shadow: Option<bool>` on Annotation) and white outline halo (`outline: Option<bool>`) for text and callout annotations. The system SHALL support the `pink` semantic style (Skitch magenta).

#### Scenario: Parse shadow and outline properties
- **WHEN** JSON specifies root scene `"shadow": false` and an annotation with `"outline": true` and `"style": "pink"`
- **THEN** the models retain the shadow flag, outline flag, and Pink semantic style
