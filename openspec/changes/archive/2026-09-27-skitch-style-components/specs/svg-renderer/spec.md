## ADDED Requirements

### Requirement: Skitch Visual Elements and Outlined Text Rendering
The SVG renderer SHALL render Skitch-style visual elements:
- Text elements with `outline: true` (or default for labels) SHALL be rendered with a thick white outline (`stroke="#ffffff" stroke-width="4" stroke-linejoin="round" paint-order="stroke fill"`).
- `pin` annotations SHALL be rendered as a pointer tag with an icon/badge circle, a directional pointed tip, and an optional linked dark/colored text pill.
- `bullseye` annotations SHALL be rendered with concentric rings and a center dot.
- `divider` annotations SHALL be rendered as a prominent colored divider line.
- Drop shadow filters (`filter="url(#markits-shadow)"`) SHALL be applied only to elements where shadow is enabled, and omitted when disabled.

#### Scenario: Render outlined text
- **WHEN** a label or text annotation specifies `outline: true`
- **THEN** output SVG `<text>` element includes `stroke="#ffffff"`, `stroke-width="4"`, and `paint-order="stroke fill"`

#### Scenario: Render pin callout tag
- **WHEN** a `pin` annotation is rendered
- **THEN** output SVG contains the pin badge, directional tip path, and attached text pill

#### Scenario: Render bullseye focus marker
- **WHEN** a `bullseye` annotation is rendered
- **THEN** output SVG contains concentric circles and a center focal dot
