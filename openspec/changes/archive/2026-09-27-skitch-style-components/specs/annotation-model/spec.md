## ADDED Requirements

### Requirement: Skitch Style Annotation Types
The system SHALL support deserializing additional visual annotation types:
- `pin` (or `pin-callout`): a pointer tag containing an icon or text badge, an integrated directional arrow point, and an optional linked text pill.
- `bullseye`: a focus target marker featuring concentric rings with a centered point.
- `divider`: a divider line spanning across a specified coordinate region.

#### Scenario: Parse pin callout annotation
- **WHEN** JSON specifies an annotation of type `pin` with target `[100, 200, 50, 50]`, icon "♡", text "お気に入り", and style `pink`
- **THEN** the system parses the attributes into a Pin annotation model

#### Scenario: Parse bullseye annotation
- **WHEN** JSON specifies an annotation of type `bullseye` with target `[300, 300, 60, 60]`
- **THEN** the system parses it into a Bullseye annotation model

#### Scenario: Parse divider annotation
- **WHEN** JSON specifies an annotation of type `divider` with target or coordinate range
- **THEN** the system parses it into a Divider annotation model

### Requirement: Drop Shadow and Text Outline Options
The system SHALL support configuring drop shadow visibility (`shadow: bool` on Scene, `shadow: Option<bool>` on Annotation) and white outline halo (`outline: Option<bool>`) for text and callout annotations. The system SHALL support the `pink` semantic style (Skitch magenta).

#### Scenario: Parse shadow and outline properties
- **WHEN** JSON specifies root scene `"shadow": false` and an annotation with `"outline": true` and `"style": "pink"`
- **THEN** the models retain the shadow flag, outline flag, and Pink semantic style
