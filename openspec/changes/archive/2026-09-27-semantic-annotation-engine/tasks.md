## 1. Project Setup & Dependencies

- [x] 1.1 Add required dependencies (`serde`, `serde_json`, `clap`, `thiserror`) to `Cargo.toml` and verify `cargo check` compiles without errors.
- [x] 1.2 Scaffold library modules (`model`, `layout`, `renderer`, `theme`, `error`) in `src/` and verify module exports compile in `lib.rs`.

## 2. Data Models & JSON Serialization

- [x] 2.1 Implement `Canvas` and `TargetRect` with custom Serde deserialization supporting both object (`{"x": ..., "width": ...}`) and 4-tuple array (`[x, y, w, h]`) formats, and verify with unit tests.
- [x] 2.2 Implement `SemanticStyle` and `PositionHint` enums with default fallbacks, and verify unit tests deserializing explicit and default values.
- [x] 2.3 Implement `Annotation` enum supporting all 8 types (`arrow`, `rect`, `rounded-rect`, `circle`, `label`, `callout`, `badge`, `spotlight`) and root `Scene` struct, verifying successful JSON deserialization across all variants.
- [x] 2.4 Implement input validation (rejecting negative canvas dimensions, negative sizes, or invalid schemas) and verify descriptive error output via unit tests.

## 3. Layout Engine

- [x] 3.1 Implement label dimension estimation based on character count and padding, and verify estimated bounding box calculations with unit tests.
- [x] 3.2 Implement candidate placement generator producing 8 anchor positions surrounding a target rectangle, and verify coordinates with unit tests.
- [x] 3.3 Implement penalty scoring for canvas boundary overflow, target collision, peer annotation collision, and position hint weighting, and verify scoring rankings with unit tests.
- [x] 3.4 Implement deterministic placement selection picking the highest-scoring candidate, and verify repeated calls yield identical placements.
- [x] 3.5 Implement pointer and arrow routing calculating connection points between callout boxes and target perimeter, and verify vector calculations with unit tests.

## 4. SVG Renderer & Themes

- [x] 4.1 Implement `Theme` system mapping semantic styles (`primary`, `secondary`, `warning`, `danger`, `info`, `step`) to concrete color and style tokens, and verify token mappings with unit tests.
- [x] 4.2 Implement root `<svg>` generator with transparent background, canvas dimensions, `viewBox`, and `<defs>` for arrowhead markers, and verify markup structure.
- [x] 4.3 Implement shape renderers for `rect`, `rounded-rect`, `circle`, `arrow`, `label`, and `badge`, verifying valid SVG element markup via unit tests.
- [x] 4.4 Implement `callout` rendering (grouped label box, text, and pointer path) and `spotlight` mask rendering (dark overlay and cut-out mask), and verify SVG output.
- [x] 4.5 Connect layout engine to SVG renderer in `Scene::render_svg()`, verifying complete SVG generation for a multi-annotation scene.

## 5. CLI Implementation

- [x] 5.1 Implement `render` subcommand in `src/main.rs` using `clap` to read JSON from a file and write SVG to stdout, and verify CLI execution with a sample file.
- [x] 5.2 Implement standard input reading when `-` is specified (`markits render -`), and verify piped input execution.
- [x] 5.3 Implement error handling for missing files, invalid JSON, or validation failures, verifying helpful stderr messages and non-zero exit codes.

## 6. Integration Testing & Verification

- [x] 6.1 Add integration tests covering complex multi-annotation scenes with callouts, badges, and spotlight masks, verifying valid SVG output matching canvas specifications.
