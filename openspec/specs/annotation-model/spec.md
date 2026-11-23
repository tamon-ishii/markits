# Annotation Model Specification

## Purpose

Provides data models, semantic preprocessing, validation, and JSON serialization/deserialization for canvases, targets, UIMaps, semantic styles, positioning hints, and all MarkIts annotation types.

## Requirements

### Requirement: Canvas and Target Geometry Parsing
The system SHALL parse canvas dimensions (`width`, `height`) and target bounding boxes (`x`, `y`, `width`, `height`) from JSON. Target rectangles MUST accept either an object (`{"x": 0, "y": 0, "width": 100, "height": 50}` with optional `w`/`h` aliases) or a 4-element array (`[x, y, width, height]`), and 2D points MUST accept `[x, y]` or `{"x": 0, "y": 0}`.

#### Scenario: Parse canvas and target object format
- **WHEN** valid JSON with canvas width 1920, height 1080, and target object `{"x": 820, "y": 640, "width": 100, "height": 32}` is provided
- **THEN** the system parses the canvas dimensions and target bounding box into strongly-typed geometry models

#### Scenario: Parse target 4-element array format
- **WHEN** valid JSON contains target as array `[820, 640, 100, 32]`
- **THEN** the system interprets the values respectively as `x=820`, `y=640`, `width=100`, `height=32`

### Requirement: Core and Extended Annotation Types
The system SHALL support deserializing all MarkIts annotation types: `arrow`, `bezier-arrow`, `rect`, `rounded-rect`, `circle` (alias `ellipse`), `label`, `callout`, `badge`, `step-arrow`, `spotlight`, `mosaic`, `pin`, `bullseye`, and `divider`.

#### Scenario: Parse callout and spotlight annotations
- **WHEN** JSON specifies annotations of type `callout` and `spotlight` with target bounding boxes
- **THEN** the system constructs typed `Callout` and `Spotlight` annotation models retaining their target, text, and style attributes

#### Scenario: Parse bezier-arrow, step-arrow, and mosaic annotations
- **WHEN** JSON specifies `bezier-arrow` with explicit `start`/`control`/`end` points, `step-arrow` with `step`, and `mosaic` with `block_size`
- **THEN** the system constructs the corresponding typed models with their geometry and styling fields

#### Scenario: Parse Skitch-style pin, bullseye, and divider annotations
- **WHEN** JSON specifies `pin` (with optional `icon` and `text`), `bullseye`, and `divider` annotations
- **THEN** the system parses each into its corresponding annotation model

### Requirement: Semantic Styles and Position Hints
The system SHALL support seven semantic styles (`primary`, `secondary`, `warning`, `danger`, `info`, `step`, `pink`) and ten position hints (`auto`, `top`, `bottom`, `left`, `right`, `top-left`, `top-right`, `bottom-left`, `bottom-right`, `center`). When omitted, `style` SHALL default to `primary` and `position` SHALL default to `auto`.

#### Scenario: Explicit semantic style and position hint
- **WHEN** an annotation defines `"style": "pink"` and `"position": "top-right"`
- **THEN** the parsed model sets style to `Pink` and position hint to `TopRight`

#### Scenario: Default style and position hint fallback
- **WHEN** an annotation JSON omits `style` and `position`
- **THEN** the parsed model defaults to `primary` style and `auto` position hint

### Requirement: Named Targets and UIMap Resolution
The system SHALL support top-level `targets` maps and `uimap` (aliases `ui_map`, `ui_elements`) arrays so annotations can reference targets by string name, role-prefixed name, suffix-trimmed label, or 1-based UIMap index.

#### Scenario: Resolve annotation target from UIMap name or index
- **WHEN** a scene includes a `uimap` element named `"保存"` at index 1 and an annotation specifies `"target": "保存ボタン"` or `"target": "1"`
- **THEN** semantic preparation resolves the string reference to the matching element's bounding box

### Requirement: High-Level Semantic Instructions
The system SHALL support the `instruction` annotation type with `action` values `click`, `enter`, `select`, `drag`, `attention`, `warning`, and `compare`, expanding each instruction into focus marks and a callout while preserving `outline`, `shadow`, and `max_width` overrides.

#### Scenario: Preserve visual overrides on an instruction
- **WHEN** an `instruction` annotation specifies `"action": "click"`, `"outline": false`, and `"shadow": false`
- **THEN** semantic preparation expands it into a spotlight focus mark and a callout retaining `"outline": false` and `"shadow": false`

#### Scenario: Require destination for drag and compare instructions
- **WHEN** an `instruction` specifies `"action": "drag"` or `"action": "compare"` without a `destination`
- **THEN** validation fails with an error stating that `destination` is required

### Requirement: Visual Styling, Arrow Options, and Privacy Redaction
The system SHALL support visual fields including scene/annotation `shadow`, `outline`, `stroke_width`, shape `line_style` (`solid`, `dashed`, `dotted`) and `fill_style` (`none`, `light`, `solid`), arrow `arrow_skin` (`classic`, `sketch`, `bold`), `arrowhead` (`filled`, `open`), `text_placement` (`middle`, `end`), `boxed`, `font_family`, `font_size`, `callout.target_box`, `badge.arrow`/`target_box`, `label.at`, `max_width`, and `mosaic.block_size` (2..=64, default 12).

#### Scenario: Parse shadow, outline, and arrow styling fields
- **WHEN** JSON specifies root `"shadow": false` and an `arrow` with `"arrow_skin": "sketch"`, `"text_placement": "end"`, and `"outline": true`
- **THEN** the parsed models retain each explicit override

### Requirement: Scene Visibility State
The system SHALL support an optional `hidden_annotations` array of 0-based annotation indices on `Scene`, preserving hidden entries in serialized data while excluding them from visible rendering via `visible_scene()`.

#### Scenario: Exclude hidden annotations from visible scene
- **WHEN** a scene has two annotations and `"hidden_annotations": [1]`
- **THEN** `visible_scene()` retains only annotation 0 while full serialization preserves both annotations and `hidden_annotations`

### Requirement: Input Validation and Error Handling
The system SHALL reject malformed JSON, unknown fields, unknown annotation types, zero canvas dimensions, non-positive or non-finite target dimensions, arrows missing both `target` and `start`/`end`, and `max_width` below 40 with descriptive error messages.

#### Scenario: Reject zero canvas or non-positive target dimensions
- **WHEN** JSON specifies canvas width `0` or a target with width `<= 0`
- **THEN** parsing fails with a descriptive validation error

#### Scenario: Reject unknown annotation type or field
- **WHEN** JSON specifies an unsupported annotation type or unknown property
- **THEN** parsing fails with an error identifying the unsupported type or field
