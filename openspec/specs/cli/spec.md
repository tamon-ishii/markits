# CLI Specification

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
