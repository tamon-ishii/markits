## Purpose

Provides capabilities to serialize MarkIts semantic annotation JSON directly into PNG image metadata chunks, extract and restore annotations from saved files for seamless re-editing, and manage clipboard copy and export.

## ADDED Requirements

### Requirement: Embed Annotation Data into PNG Metadata
The system SHALL embed the serialized MarkIts semantic annotation JSON model into a standard PNG ancillary text chunk (such as `tEXt` or `zTXt` with key `markits:annotations`) when saving the annotated image as a PNG file.

#### Scenario: Exporting annotated PNG with embedded scene data
- **WHEN** user saves an annotated capture to a PNG file
- **THEN** the system renders the composed image into PNG raster bytes and writes the complete MarkIts JSON scene definition into the PNG metadata chunk

#### Scenario: File remains viewable in standard image viewers
- **WHEN** an annotated PNG containing MarkIts metadata is opened in a standard third-party image viewer or web browser
- **THEN** the image renders identically to a normal PNG without displaying metadata artifacts or corrupting image display

### Requirement: Restore Annotations from PNG Metadata for Re-editing
The system SHALL inspect opened PNG image files for MarkIts metadata chunks, extract the embedded annotation definitions, and reconstruct the editable canvas scene with all original marks and handles intact.

#### Scenario: Re-editing a previously saved MarkIts PNG
- **WHEN** user opens a PNG file previously created by MarkIts Desktop into the editor
- **THEN** the system extracts the background image and restores all annotations into the editor, allowing handles and text to be edited again

#### Scenario: Opening an image without MarkIts metadata
- **WHEN** user opens an image that contains no MarkIts metadata
- **THEN** the system treats the image as a plain background canvas ready for new annotations

### Requirement: Clipboard Image and Data Export
The system SHALL support copying the rendered composite image directly to the system clipboard, allowing users to paste it immediately into chat applications, documents, or bug trackers.

#### Scenario: Copy to clipboard
- **WHEN** user clicks "Copy to Clipboard" or presses Ctrl+C (Cmd+C)
- **THEN** the rendered composite image is written to the system clipboard as a standard bitmap/image format and a notification confirms success
