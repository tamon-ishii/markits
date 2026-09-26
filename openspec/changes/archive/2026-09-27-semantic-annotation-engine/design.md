## Context

See `proposal.md` for motivation.
`markits` is a newly initialized Rust crate currently containing only boilerplate. This design establishes the foundational internal architecture for both the core library (`lib.rs`) and the companion CLI binary (`main.rs`).

## Goals / Non-Goals

**Goals:**
- Provide a clean, deterministic pipeline: `Semantic JSON` → `Annotation Model` → `Layout Engine` → `SVG Renderer` → `SVG String`.
- Implement heuristic, collision-aware auto-placement and arrow routing without external heavy solver dependencies.
- Render crisp, vector-based transparent SVGs with built-in semantic design themes (primary, warning, etc.).
- Expose a simple Rust library API (`Scene::render_svg()`) and an ergonomic CLI (`markits render <file|- >`).

**Non-Goals:**
- Image decoding, raster rendering, or screenshot capture (e.g. PNG/JPEG manipulation).
- AI model integration, OCR, or DOM analysis.
- Complex font rendering engines (FreeType/HarfBuzz); simple proportional/monospace metric heuristics with generous padding suffice for callout text bounding boxes.

## Decisions

### 1. Crate Architecture and Module Separation
- **Library (`markits`) modules**:
  - `model`: Core data structures (`Scene`, `Canvas`, `TargetRect`, `Annotation`, `SemanticStyle`, `PositionHint`).
  - `layout`: Layout engine (`LayoutEngine`, `CandidateGenerator`, `CollisionDetector`, `ScoringContext`, `ResolvedScene`).
  - `renderer`: SVG emission (`SvgRenderer`, `Theme`, `SvgElement`, `MaskDef`).
  - `error`: Unified error enum (`MarkitsError`) with `serde_json` and validation variants.
- **Binary (`markits`)**:
  - Uses `clap` (derive) for CLI command parsing (`RenderArgs`), reads file or stdin, invokes `markits::render_from_json`, and prints to stdout.
- *Rationale*: Clear boundary between data modeling, algorithmic placement, and markup rendering enables independent unit testing and future format additions.

### 2. Geometry & Flexible Target Serialization
- `TargetRect` struct with `{ x: f64, y: f64, width: f64, height: f64 }`.
- Custom Serde deserializer to accept both object syntax (`{"x": 100, "y": 200, "width": 50, "height": 30}`) and array syntax (`[100, 200, 50, 30]`).
- Internal calculations use `f64` for precise coordinate placement and SVG path generation.
- *Alternatives considered*: Requiring object syntax only. *Rejected* because array syntax `[x, y, w, h]` is significantly more compact and common in AI vision models and bounding box outputs.

### 3. Heuristic Deterministic Layout Engine
- For each target-dependent annotation (e.g. `callout`, `label`, `badge`):
  1. Estimate annotation body size (width, height) using font metrics + padding.
  2. Generate candidate anchors around the target (Top, Bottom, Left, Right, TopLeft, TopRight, BottomLeft, BottomRight).
  3. Calculate penalty score for each candidate:
     - Boundary penalty (high weight if bounds exceed canvas 0..W, 0..H).
     - Target overlap penalty (critical penalty if candidate intersects the target).
     - Peer overlap penalty (high penalty if candidate intersects already-placed annotations).
     - Distance penalty (slight penalty proportional to offset distance).
     - Position hint bonus/penalty (favors user hint if specified, e.g. `top`).
  4. Select lowest penalty candidate.
  5. Compute arrow/pointer connection vectors between the selected callout box and the nearest target edge point.
- *Rationale*: A deterministic greedy heuristic with standard penalty weights is predictable, fast (sub-millisecond), requires zero external solver runtime dependencies, and satisfies all requirements.

### 4. Pure Rust Vector SVG Generation
- SVG generation implemented via pure Rust string formatting and lightweight XML building.
- No heavy C libraries (like Cairo or librsvg).
- Transparent root `<svg width="..." height="..." viewBox="...">` containing `<defs>` (arrow markers, masks, filter effects) followed by annotation layers.
- For `spotlight`: creates a `<mask id="...">` with a white canvas `<rect>` and black target cut-out `<rect rx="..." ry="...">`, then renders a `<rect fill="black" opacity="0.6" mask="url(#...)"/>`.
- *Rationale*: Ensures zero C-dependency compilation, maximum cross-platform portability (Linux, macOS, Windows, WebAssembly), and fast execution.

### 5. Semantic Theme System
- Define a `Theme` struct holding visual attributes for each `SemanticStyle`:
  - `stroke_color`, `fill_color`, `text_color`, `stroke_width`, `font_family`, `font_size`, `corner_radius`, `padding`.
- Standard styles: `primary` (blue/indigo), `secondary` (gray), `warning` (amber), `danger` (red), `info` (cyan/teal), `step` (purple/indigo with numbered badge).
- *Rationale*: Isolates visual aesthetic from semantic intent and enables future custom theme support.

## Risks / Trade-offs

- **[Risk] Text size estimation discrepancy without a native font rendering engine**
  → *Mitigation*: Use conservative character width heuristics (e.g. ~0.6 * fontSize for average latin/digits, 1.0 * fontSize for CJK) combined with generous internal box padding (12px horizontal, 6px vertical) so text never overflows the callout background.
- **[Risk] Multiple annotations crowding in dense UI screenshots**
  → *Mitigation*: Sort annotations by priority/index, layout iteratively, and if all standard 8 candidate slots collide, select the slot with minimum overlap area while strictly enforcing canvas boundaries.
- **[Risk] Crossing arrow lines in multi-callout scenarios**
  → *Mitigation*: Arrow routing connects the closest edge points between callout and target; global inter-arrow intersection detection can apply a score penalty to crossing configurations.
