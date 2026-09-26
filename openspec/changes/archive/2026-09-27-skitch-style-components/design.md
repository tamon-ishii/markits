## Context

See `proposal.md` for motivation.
This change adds high-visibility Skitch-style visual components (outlined text with halo, integrated pin/pointer tags, bullseye focus circles, divider lines, and Skitch pink theme color) while incorporating drop shadow configurability.

## Goals / Non-Goals

**Goals:**
- Implement `pin` (tag with icon/badge + pointer tip + optional linked text pill).
- Implement `bullseye` (concentric rings + focus center point).
- Implement `divider` (accent boundary/section line).
- Support white text outlines via SVG `paint-order="stroke fill"`.
- Support global and per-element drop shadow toggles (`shadow: bool`).
- Add iconic `pink` semantic style (`#ea1a65`).

**Non-Goals:**
- Freehand pencil/highlighter drawing.

## Decisions

### 1. Model Extensions
- `SemanticStyle`: Add `Pink` variant (`#ea1a65`).
- `Scene`: Add `#[serde(default = "default_true")] pub shadow: bool`.
- `Annotation` enum additions:
  - `Pin { target: TargetRect, icon: Option<String>, text: Option<String>, style: SemanticStyle, position: PositionHint, shadow: Option<bool>, outline: Option<bool> }`
  - `Bullseye { target: TargetRect, style: SemanticStyle }`
  - `Divider { target: TargetRect, style: SemanticStyle }`
- Add `shadow: Option<bool>` and `outline: Option<bool>` to existing text/callout annotations.

### 2. Layout Geometry
- `Pin`:
  - Head circle radius ~18px.
  - Pointer tip extends ~10px towards the target edge.
  - Attached text pill (if `text` present) placed immediately adjacent to the pin head on the side opposite the pointer.
- `Bullseye`:
  - Centered at target `(center_x, center_y)`.
  - Outer ring radius ~24px, inner ring radius ~14px, center dot radius ~4px.
- `Divider`:
  - Spans horizontally across target width (or canvas width).

### 3. SVG Rendering
- Outlined Text:
  - `<text stroke="#ffffff" stroke-width="4" stroke-linejoin="round" paint-order="stroke fill" fill="{color}">`
  - Guarantees readability on light and dark image backgrounds.
- Pin Tag:
  - Smooth compound path connecting the circular badge and triangular arrow pointer tip.
  - Centered icon/symbol in white or contrasting color.
  - Adjacent pill rect with text.
- Conditional Drop Shadow:
  - Applied only when resolved `shadow == true`.
