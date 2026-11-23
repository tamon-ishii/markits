# image-metadata Specification

## Purpose

Defines privacy-first export metadata policy, PNG and XMP metadata container embedding for PNG/WebP/JPEG XL, full scene and crop restoration, lossless metadata stripping, and clipboard/JSON export.

## Requirements

### Requirement: Privacy-First Default Export and Opt-In Editable Metadata
Ordinary desktop file saves and CLI image exports SHALL strip all `markits:*` metadata by default so exported files do not leak original uncropped pixels, UIMaps, or operation history. Full editable metadata SHALL be embedded only for local capture history, external integration saves with editable metadata enabled, and CLI commands invoked with `--editing-metadata` (or explicit UIMap/recipe capture flags).

#### Scenario: Ordinary desktop save strips editable metadata
- **WHEN** user saves an annotated image via the editor's standard Save action
- **THEN** the saved image contains the flattened pixels (including any visible mosaic redactions) and zero `markits:*` metadata chunks or payloads

#### Scenario: Opt-in editable export retains metadata
- **WHEN** user runs CLI export/render with `--editing-metadata` or completes an integration save with editable metadata enabled
- **THEN** the saved file embeds the editable scene, source image, UIMap, crop info, and operation metadata

### Requirement: PNG and XMP Container Metadata Embedding
When embedding metadata, the system SHALL store `markits:*` keys (`markits:annotations`, `markits:ui_elements`, `markits:crop_info`, `markits:source_image`, `markits:base_image`, `markits:capture_target`, `markits:operations`, `markits:operation_group`, `markits:capture_recipe`) in PNG `tEXt` chunks, or in a versioned base64 JSON `<markits:payload>` inside a WebP `XMP ` chunk or JPEG XL `xml ` box, while keeping images viewable in standard viewers.

#### Scenario: Round-trip editable WebP and JPEG XL exports
- **WHEN** an editable image with MarkIts metadata is saved as `.webp` or `.jxl` and reopened
- **THEN** pixels decode losslessly and all embedded `markits:*` metadata keys are restored

#### Scenario: File remains viewable in standard image viewers
- **WHEN** an image containing MarkIts metadata is opened in a third-party viewer or browser
- **THEN** it renders identically to a normal image without visual corruption

### Requirement: Restore Editable Scene and Crop State from Metadata
When opening an image containing MarkIts metadata, the system SHALL restore the unannotated source background, annotations in their original coordinate system (even when exported at a resized resolution), UIMap elements, and reversible crop base state; images without MarkIts metadata SHALL open as plain canvases.

#### Scenario: Re-editing a previously saved editable MarkIts image
- **WHEN** user opens an editable MarkIts PNG, WebP, or JPEG XL (including a resized or cropped export)
- **THEN** the editor restores the unannotated background dimensions, crop base state, UIMap, and editable marks without double-drawing annotations

#### Scenario: Opening a plain image without metadata
- **WHEN** user opens an image containing no MarkIts metadata
- **THEN** the editor treats it as a plain background canvas ready for new annotations

### Requirement: Lossless Metadata Stripping
The system SHALL strip all `markits:*` PNG text chunks (`tEXt`, `zTXt`, `iTXt`) and `<markits:payload>` segments in WebP/JPEG XL XMP boxes without decoding or re-encoding compressed image pixels and without removing unrelated metadata such as color profiles or author tags.

#### Scenario: Strip MarkIts metadata losslessly
- **WHEN** `strip_markits_metadata` runs on a PNG, WebP, or JPEG XL containing MarkIts metadata and an unrelated `Author` tag
- **THEN** all `markits:*` data is removed, unrelated metadata is preserved, and pixel values remain identical

### Requirement: Clipboard Image Export and Optional Reproduction JSON
The system SHALL support copying the rendered composite bitmap to the system clipboard and SHALL support optionally saving a companion reproduction JSON file alongside standard desktop saves using UI element names for snapped targets.

#### Scenario: Copy composite image to clipboard
- **WHEN** user clicks "Copy" in the editor
- **THEN** the rendered composite image at the selected output dimensions is written to the system clipboard
