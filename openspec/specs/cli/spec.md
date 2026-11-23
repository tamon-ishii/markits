# CLI Specification

## Purpose

Provides the `markits` command-line interface for rendering semantic SVG and annotated images, capturing screens and windows, inspecting and extracting UIMaps, cropping, converting/stripping metadata, managing history, and recording/replaying automated capture recipes.

## Requirements

### Requirement: SVG Render Subcommand
The CLI SHALL provide `render <input>` (file path or `-` for stdin) to emit SVG to stdout, with optional JSON layout geometry (`--layout-json`), layout debug overlays (`--debug`), and external UIMap loading (`--uimap`).

#### Scenario: Render SVG from file or stdin
- **WHEN** user executes `markits render annotation.json` or `cat annotation.json | markits render -`
- **THEN** the system prints generated SVG markup to stdout and exits with code 0

### Requirement: Composited Image Render Mode
When `render` receives `--image` and `--output`, the CLI SHALL composite annotations onto PNG, JPEG, WebP, or JPEG XL inputs and save PNG, WebP, or JPEG XL outputs with optional `--crop`/`--crop-margin` and `--editing-metadata`, inferring omitted `canvas` dimensions from the image and rejecting mismatched explicit canvas dimensions.

#### Scenario: Render annotated image with inferred canvas
- **WHEN** user executes `markits render annotations.json --image screenshot.png --output annotated.webp` and the JSON omits `canvas`
- **THEN** the system infers canvas dimensions from `screenshot.png` and writes the composited image

#### Scenario: Reject mismatched canvas dimensions
- **WHEN** JSON specifies canvas dimensions differing from `--image`
- **THEN** the system reports the mismatch to stderr, exits non-zero, and writes no output file

### Requirement: Validate, Inspect, and UIMap Subcommands
The CLI SHALL provide `validate` (with optional `--image` and `--uimap`), `inspect` (reporting `width`, `height`, `format`, `has_uimap`, `has_capture_recipe`, `uimap_elements_count`, and optional `--uimap` elements as JSON for PNG/JPEG/WebP/JXL), and `uimap` (listing, filtering by `--filter`, or exporting embedded UIMaps with `--json`/`--output`).

#### Scenario: Inspect an image and extract its UIMap
- **WHEN** user executes `markits inspect screenshot.png` or `markits uimap screenshot.png --json --filter button`
- **THEN** the system outputs structured JSON metadata or filtered UIMap elements to stdout

#### Scenario: Validate annotations against an image
- **WHEN** user executes `markits validate annotations.json --image screenshot.png`
- **THEN** the system validates the document using the image dimensions and exits non-zero if invalid or mismatched

### Requirement: Quick Annotation and Batch Workflows
The CLI SHALL provide `annotate` for applying a single mark via flags (`--target`, `--mark`, `--text`, `--step`, `--style`, `--position`, `--uimap`, `--crop`) and `annotate-batch` for applying a JSON array of marks with an optional JSON `--template` of default style fields. Reusing a UIMap image whose dimensions differ from the target image SHALL print a warning to stderr.

#### Scenario: Apply batch marks with style template
- **WHEN** user executes `markits annotate-batch screen.png marks.json --template defaults.json -o out.png`
- **THEN** supported template defaults fill omitted fields on each mark and the composited image is written

#### Scenario: Warn when reusing mismatched UIMap image
- **WHEN** `--uimap` references an image whose dimensions differ from the target image
- **THEN** the CLI prints a warning to stderr while continuing processing

### Requirement: Capture, Capture Series, and Crop Subcommands
The CLI SHALL provide `capture` (listing screens/windows, capturing screen/window/PID/region with optional `--include-cursor`, `--detect-ui`, `--uimap`, and immediate `--target` annotation), `capture-series` (capturing `1..=1000` numbered PNGs with bounded `--delay-ms` and `--interval-ms` up to one hour), and `crop` (pixel cropping by `--x`, `--y`, `--width`, `--height`).

#### Scenario: Capture a bounded timed series
- **WHEN** user executes `markits capture-series shot.png --count 3 --delay-ms 0 --interval-ms 100`
- **THEN** the system writes `shot_001.png`, `shot_002.png`, and `shot_003.png` and rejects `--count 0` or `--count > 1000`

### Requirement: Export, Metadata Stripping, and History Subcommands
The CLI SHALL provide `export` (converting between PNG, WebP, and JPEG XL, stripping metadata unless `--editing-metadata` is set), `strip-metadata` (losslessly removing MarkIts metadata while rejecting same-file overwrite), and `history` (`list`, `load`, `save`, `delete`) for desktop history management.

#### Scenario: Strip metadata without overwriting source
- **WHEN** user executes `markits strip-metadata in.png -o out.png` versus pointing `-o` to `in.png`
- **THEN** writing to a distinct output removes all MarkIts metadata losslessly, while targeting the same file fails with an error

### Requirement: Recording, Replay, and Capture Recipes
The CLI SHALL provide `record` (`start`, `status`, `pause`, `resume`, `screenshot`, `finish`, `cancel`), `replay` (with `--dry-run`, `--shorten-waits`, `--max-wait-ms`, `--include-cursor`), and reproducible recipe commands `capture-run`, `recapture`, and `recipe` (extracting recipes without executing them, and supporting `--dry-run` inspection).

#### Scenario: Inspect embedded capture recipe without execution
- **WHEN** user executes `markits recipe captured.png` or `markits recapture captured.png --dry-run`
- **THEN** the CLI prints the saved recipe JSON without launching external executables or sending input

### Requirement: Bundled AI Manual, Global Options, and Error Reporting
The CLI SHALL advertise `markits manual` in `markits -h`, print the bundled English Markdown manual on `markits manual`, accept global `--ui-depth` (`1..=128`) and `--editing-metadata`, and report invalid inputs or I/O errors to stderr with a non-zero exit code.

#### Scenario: Agent discovers usage without repository files
- **WHEN** an agent runs `markits -h` and then `markits manual`
- **THEN** it receives the complete CLI workflow and JSON examples via stdout

#### Scenario: Malformed JSON input error
- **WHEN** user provides syntactically invalid JSON or a missing file to `markits render`
- **THEN** the CLI writes a descriptive error message to stderr and exits with a non-zero code
