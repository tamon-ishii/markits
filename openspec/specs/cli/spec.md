# CLI Specification

## Batch workflows

The CLI SHALL support `annotate-batch` with a JSON array of marks and an optional JSON template of default style fields. It SHALL support `capture-series` with a bounded count, initial delay, and interval. When reusing a PNG UIMap whose dimensions differ from the target image, it SHALL warn on stderr.

## Purpose

Provides a command-line interface for reading semantic annotation JSON from files or standard input and rendering SVG to standard output.

## Requirements

### Requirement: Render Subcommand with File Input
The system SHALL provide a `render` subcommand accepting a file path to an annotation JSON file and writing the rendered SVG output to standard output (`stdout`).

#### Scenario: Successfully render SVG from file
- **WHEN** user executes `markits render annotation.json`
- **THEN** system prints generated SVG markup to stdout and exits with code 0

#### Scenario: Handle missing or inaccessible input file
- **WHEN** user executes `markits render nonexistent.json`
- **THEN** system prints an error message to stderr and exits with non-zero code

### Requirement: Standard Input Support
The system SHALL support reading annotation JSON from standard input (`stdin`) when the file argument is specified as `-`.

#### Scenario: Render SVG from piped stdin
- **WHEN** user executes `cat annotation.json | markits render -`
- **THEN** system reads JSON from stdin, writes SVG markup to stdout, and exits with code 0

### Requirement: Error Reporting on Invalid JSON
The system SHALL report informative error messages to standard error (`stderr`) and exit with a non-zero exit code when the input JSON is malformed or fails schema validation.

#### Scenario: Malformed JSON input error
- **WHEN** user provides syntactically invalid JSON to `markits render`
- **THEN** system outputs a syntax error message to stderr and terminates with a non-zero exit code

### Requirement: Annotated Image Output
The system SHALL accept a PNG or JPEG image with annotation JSON and save a composited PNG when `render` receives `--image` and `--output`. In image mode, the system SHALL derive canvas dimensions from the source image when JSON omits `canvas`, and SHALL reject an explicit canvas whose dimensions differ from the image.

#### Scenario: Render annotated PNG with inferred canvas
- **WHEN** user executes `markits render annotations.json --image screenshot.png --output annotated.png` and the JSON omits `canvas`
- **THEN** the system writes a PNG with the source image's dimensions and the annotations above the image

#### Scenario: Reject mismatched canvas
- **WHEN** JSON canvas dimensions differ from the input image dimensions
- **THEN** the system reports the mismatch to stderr, exits non-zero, and does not write a new image

### Requirement: Image Inspection and Validation
The system SHALL provide `inspect` to report PNG or JPEG format and pixel dimensions as JSON. The `validate` subcommand SHALL accept `--image` to infer or verify canvas dimensions.

#### Scenario: Inspect an image
- **WHEN** user executes `markits inspect screenshot.png`
- **THEN** the system writes JSON containing `width`, `height`, and `format` to stdout

#### Scenario: Validate annotations against an image
- **WHEN** user executes `markits validate annotations.json --image screenshot.png`
- **THEN** the system validates annotations using the image dimensions and reports an error if an explicit canvas differs

### Requirement: Bundled AI Manual
The system SHALL point to `markits manual` in `markits -h`, and `markits manual` SHALL print the bundled English Markdown manual from the installed binary.

#### Scenario: Agent discovers usage without repository files
- **WHEN** an agent runs `markits -h` and then `markits manual`
- **THEN** it receives the image annotation workflow and JSON examples through standard output
