# screen-capture Specification

## Purpose

Provides multi-mode screen capture for the desktop app and CLI, including window selection, rectangular region selection, full screen capture, cursor-included delayed capture, clipboard import, and recording target selection.

## Requirements

### Requirement: Window Selection Overlay
The desktop capture overlay SHALL start in window selection mode on a dimmed screen snapshot, highlight the topmost window under the cursor in Z-order with a clear cutout, and capture that window on a single click or Enter key. Pressing Escape, right-clicking, or clicking Cancel SHALL dismiss the overlay without capturing.

#### Scenario: Hovering and selecting an application window
- **WHEN** user moves the cursor over an open window in window selection mode and clicks it
- **THEN** the hovered window is highlighted with a cutout outline and clicking captures only that window's region into the editor

#### Scenario: Canceling capture
- **WHEN** user presses Escape or right-clicks while the capture overlay is active
- **THEN** the overlay is dismissed immediately without taking a capture

### Requirement: Rectangular Region Selection Mode
The desktop capture overlay SHALL provide a toolbar toggle to enter rectangular region selection mode, allowing users to click and drag an arbitrary bounding box (at least 2x2 pixels) with live cutout dimming and capture it on mouse release.

#### Scenario: Drag selection of screen region
- **WHEN** user switches the overlay to region mode, drags a rectangle of at least 2x2 pixels, and releases the mouse button
- **THEN** the system crops the screen image to that region, dismisses the overlay, and opens the editor (or arms cursor capture if cursor inclusion is enabled)

### Requirement: Full Screen Capture
The system SHALL allow capturing the entire desktop screen (or selected monitor via CLI) directly without manual window or region cropping.

#### Scenario: Capture full screen from overlay toolbar
- **WHEN** user clicks the Full Screen action in the capture overlay toolbar
- **THEN** the full desktop image is loaded into the editor (or armed for cursor capture if cursor inclusion is enabled)

### Requirement: Cursor-Included Capture Mode
On supported platforms (Windows), the capture overlay SHALL offer an independent "Include Mouse Cursor" checkbox whose toggle never initiates capture. Selecting a window, region, or full screen while enabled SHALL dismiss the overlay, display a floating guide showing the configured capture shortcut, and capture live pixels plus the mouse cursor when the user presses that shortcut again, preserving waiting state on failure for retry until explicitly canceled.

#### Scenario: Capture hover state with cursor
- **WHEN** user enables "Include Mouse Cursor", clicks a target window, moves the pointer to trigger hover UI, and presses the displayed shortcut
- **THEN** MarkIts hides the guide, captures the live window pixels with the cursor rendered, records pointer metadata, and opens the editor

#### Scenario: Toggle cursor checkbox without starting capture
- **WHEN** user toggles the "Include Mouse Cursor" checkbox in the overlay toolbar
- **THEN** only the option state and instruction hint update without selecting a window or starting capture

### Requirement: Clipboard Image Import
The desktop application SHALL allow importing an image directly from the system clipboard into the annotation editor.

#### Scenario: Import image from clipboard
- **WHEN** user clicks "Capture from Clipboard" while an image is in the system clipboard
- **THEN** the clipboard image opens directly in the editor ready for annotation

### Requirement: Recording Target Selection Mode
When started for operation recording, the capture overlay SHALL restrict selection to a target window, preserve the cursor inclusion option, and launch operation recording for the chosen window.

#### Scenario: Select window to start operation recording
- **WHEN** user starts operation recording from Home and clicks a valid application window in the overlay
- **THEN** the overlay closes and operation recording begins for that window with the chosen cursor option

### Requirement: CLI Multi-Monitor, Window, and Region Capture
The CLI SHALL support enumerating screens (`--list-screens`) and top-level windows (`--list-windows`, returning an empty list safely on headless Linux), capturing by `--screen`, `--window`, `--pid`, or monitor-local `--x/--y/--width/--height` coordinates, optionally including the cursor on Windows (`--include-cursor`) and detecting or reusing UIMap metadata (`--detect-ui`, `--uimap`).

#### Scenario: CLI region on a selected monitor
- **WHEN** the CLI receives `--screen` with all four region coordinates (`--x`, `--y`, `--width`, `--height`)
- **THEN** the region is cropped from that monitor using monitor-local pixel coordinates and detected UI elements use the cropped image origin
