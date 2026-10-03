# screen-capture Specification

## Purpose

Provides multi-mode display capture capabilities including full screen, active or hovered window capture, and arbitrary rectangular area selection with an interactive translucent overlay.

## Requirements

### Requirement: Rectangular Region Selection Overlay
The system SHALL display a full-screen semi-transparent overlay allowing users to click and drag to select an arbitrary rectangular bounding box for screen capture, showing real-time dimensions and selection coordinates.

#### Scenario: Drag selection of screen region
- **WHEN** user clicks on the overlay screen and drags to a target coordinate
- **THEN** a highlighted rectangular region is drawn on top of the darkened screen with real-time width and height dimensions

#### Scenario: Confirming region capture
- **WHEN** user releases the mouse button after dragging a region larger than a minimum dimension threshold
- **THEN** the system crops the screen image to the selected bounding box, dismisses the overlay, and passes the pixel data to the editor

#### Scenario: Canceling capture
- **WHEN** user presses the Escape key or right-clicks while the capture overlay is active
- **THEN** the overlay is dismissed immediately without taking a capture or opening the editor

### Requirement: Full Screen Capture
The system SHALL allow capturing the entire desktop screen (or selected monitor in a multi-monitor configuration) without manual cropping.

#### Scenario: Capture full screen directly
- **WHEN** user initiates a full-screen capture command
- **THEN** the entire desktop canvas is captured at native display resolution and loaded directly into the editor

### Requirement: Window Selection Mode
The system SHALL detect window boundaries under the cursor and allow the user to capture an entire window with a single click.

#### Scenario: Hovering and selecting an application window
- **WHEN** user moves the cursor over an open window during window selection mode
- **THEN** the target window's bounding box is highlighted with an outline, and clicking it captures only that window's region
