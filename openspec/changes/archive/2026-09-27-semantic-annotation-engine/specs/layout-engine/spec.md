## Purpose

Calculates deterministic label positioning, collision-free placement, canvas boundary containment, and arrow routing based on target coordinates and position hints.

## ADDED Requirements

### Requirement: Candidate Placement Generation
The system SHALL generate candidate placement boxes surrounding the target rectangle (including top, bottom, left, right, top-left, top-right, bottom-left, bottom-right). When a position hint other than `auto` is provided, the system SHALL prioritize candidate positions matching the hint.

#### Scenario: Generate surrounding candidate positions
- **WHEN** layout is calculated for a callout annotation with position hint `auto`
- **THEN** candidate bounding boxes are generated across all surrounding canonical positions relative to target

#### Scenario: Prioritize hinted position
- **WHEN** a callout annotation provides position hint `top`
- **THEN** the candidate position directly above the target receives positive score weighting during selection

### Requirement: Collision and Boundary Scoring
The system SHALL evaluate each candidate position using a composite scoring function that penalizes or rejects placements extending beyond canvas boundaries or colliding with the target bounding box or other annotations.

#### Scenario: Canvas boundary containment
- **WHEN** a candidate position partially or fully falls outside canvas width and height
- **THEN** that candidate receives a heavy penalty or is disqualified from selection

#### Scenario: Overlap avoidance with other annotations
- **WHEN** multiple annotations are laid out simultaneously and one candidate position intersects another annotation's bounding box
- **THEN** the overlapping candidate receives a collision penalty prioritizing non-overlapping candidates

### Requirement: Deterministic Optimal Placement Selection
The layout engine SHALL deterministically select the candidate with the highest composite score given identical inputs.

#### Scenario: Deterministic resolution
- **WHEN** the layout engine processes the same scene configuration multiple times
- **THEN** identical coordinates and positions are selected each time

### Requirement: Arrow and Pointer Routing
For annotations requiring pointers or arrows (such as callouts and arrows), the system SHALL calculate start and end connection points between the annotation body and the target boundary edge.

#### Scenario: Route pointer from callout to target
- **WHEN** a callout is placed at the top of a target rectangle
- **THEN** the system determines an arrow start point at the bottom edge of the callout box and an end point at the top edge of the target rectangle

#### Scenario: Prevent pointer intersection across multiple annotations
- **WHEN** two callouts point to distinct targets
- **THEN** routing selects connection anchors minimizing line intersections
