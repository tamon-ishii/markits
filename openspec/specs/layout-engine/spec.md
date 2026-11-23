# Layout Engine Specification

## Purpose

Calculates deterministic annotation placement, text wrapping, collision-free positioning, canvas boundary containment, arrow routing, and resolved element geometry from validated scenes.

## Requirements

### Requirement: Candidate Placement Generation
The system SHALL generate candidate placement boxes surrounding the target rectangle (`top`, `bottom`, `left`, `right`, `top-left`, `top-right`, `bottom-left`, `bottom-right`). When a position hint other than `auto` is provided, the system SHALL prioritize candidate positions matching the hint.

#### Scenario: Generate surrounding candidate positions
- **WHEN** layout is calculated for a callout annotation with position hint `auto`
- **THEN** candidate bounding boxes are generated across all surrounding canonical positions relative to the target

#### Scenario: Prioritize hinted position
- **WHEN** a callout annotation provides position hint `top`
- **THEN** the candidate position directly above the target receives positive score weighting during selection

### Requirement: Collision and Boundary Scoring
The system SHALL evaluate each candidate position using a composite scoring function that penalizes placements extending beyond canvas boundaries or colliding with the target bounding box or already placed annotations.

#### Scenario: Canvas boundary containment
- **WHEN** a candidate position partially or fully falls outside canvas width and height
- **THEN** that candidate receives a heavy boundary penalty favoring in-bounds candidates

#### Scenario: Overlap avoidance with other annotations
- **WHEN** multiple annotations are laid out and a candidate intersects an occupied annotation's bounding box
- **THEN** the overlapping candidate receives a collision penalty prioritizing non-overlapping candidates

### Requirement: Deterministic Optimal Placement Selection
The layout engine SHALL deterministically select the candidate with the highest composite score given identical inputs.

#### Scenario: Deterministic resolution
- **WHEN** the layout engine processes the same scene configuration multiple times
- **THEN** identical coordinates and positions are selected each time

### Requirement: Text Measurement and Wrapping
The layout engine SHALL estimate text box dimensions accounting for ASCII and wider CJK characters and SHALL wrap label and callout text to `max_width` (or a canvas-relative default cap) by breaking Latin text at spaces, CJK text between characters, and preserving explicit newlines.

#### Scenario: Wrap mixed-language callout text
- **WHEN** a callout specifies long text exceeding `max_width`
- **THEN** the layout engine inserts line breaks within the width limit while preserving explicit `\n` breaks

### Requirement: Arrow, Bezier Arrow, and Pointer Routing
The system SHALL compute connection points for callouts, step-arrows, straight arrows, and quadratic Bezier arrows while minimizing pointer intersections. For arrow text, `text_placement: "middle"` SHALL place the label beside the shaft or curve, and `text_placement: "end"` SHALL place the label beyond the tail end opposite the arrowhead without overlapping the line.

#### Scenario: Route pointer from callout to target
- **WHEN** a callout is placed above a target rectangle
- **THEN** the arrow starts at the callout box bottom edge and ends at the target top edge while penalizing crossings with existing pointers

#### Scenario: Place arrow label at tail end
- **WHEN** a straight or Bezier arrow specifies `text_placement: "end"`
- **THEN** the layout places the label beyond the arrow's tail (`start`) cleared by the label's projected bounds

### Requirement: Layout Resolution for Pins, Bullseyes, and Dividers
The layout engine SHALL resolve `pin` head and tip coordinates pointing at the target edge with an adjacent text pill, center `bullseye` rings on the target midpoint, and orient `divider` start/end coordinates horizontally or vertically according to the target aspect ratio.

#### Scenario: Resolve pin callout placement
- **WHEN** a `pin` annotation is placed with position hint `top`
- **THEN** the pin tip points downward to the target top edge and any attached text pill is aligned horizontally with the pin head

#### Scenario: Resolve bullseye centering
- **WHEN** a `bullseye` annotation specifies a target bounding box
- **THEN** the center point is resolved to the exact midpoint `(center_x, center_y)` of the target box

### Requirement: Visible Scene Filtering and Layout Geometry Export
The layout engine SHALL exclude annotations listed in `hidden_annotations` from resolved layout output and SHALL expose resolved bounding boxes and arrow paths as `LayoutElement` entries for `--layout-json` and auto-cropping.

#### Scenario: Omit hidden marks from layout bounds
- **WHEN** a scene marks an annotation index in `hidden_annotations`
- **THEN** that annotation is excluded from resolved layout elements and auto-crop bounding calculations
