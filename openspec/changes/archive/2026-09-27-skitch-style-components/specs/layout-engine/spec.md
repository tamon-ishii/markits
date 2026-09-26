## ADDED Requirements

### Requirement: Layout Resolution for Pins, Bullseyes, and Dividers
The layout engine SHALL resolve positions and dimensions for `pin`, `bullseye`, and `divider` annotations:
- For `pin` annotations: determine the pin head location pointing toward the target edge, and position any attached text pill adjacent to the pin head.
- For `bullseye` annotations: align the concentric rings and center point to the target center.
- For `divider` annotations: resolve start and end coordinates across the specified canvas bounds or target bounds.

#### Scenario: Resolve pin callout placement
- **WHEN** a `pin` annotation is placed with position hint `top`
- **THEN** the pin tip points downward to the target top edge and the attached text pill is placed horizontally aligned with the pin head

#### Scenario: Resolve bullseye centering
- **WHEN** a `bullseye` annotation specifies a target bounding box
- **THEN** the center point is resolved to the exact midpoint `(center_x, center_y)` of the target box
