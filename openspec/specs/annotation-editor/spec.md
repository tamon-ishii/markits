# annotation-editor Specification

## Style presets

The editor SHALL offer selectable mark style presets that update the supported semantic color, stroke width, shadow, and outline fields of the selected annotation and render the change immediately.
For straight and curved arrows, the editor SHALL also offer solid, dashed, and dotted lines and filled or open arrowheads.
For straight and curved arrows, the editor SHALL offer three arrow skins with previews: classic, hand-drawn outline with diagonal hatching throughout the shaft and head, and tapered filled arrow. The selected skin SHALL change the entire shaft and head immediately and persist in the annotation data. Line pattern and arrowhead controls apply to the classic skin.
The arrow text placement control SHALL identify whether it moves the label to the shaft's middle or near the tip, render the selected position immediately, and serialize only the canonical `text_placement` field.

## Purpose

Provides an interactive graphical canvas editor for placing, manipulating handles, customizing typography, and applying semantic styles to MarkIts annotations on top of captured images.

## Requirements

### Requirement: Interactive Mark Tool Palette
The system SHALL provide a toolbar offering MarkIts annotation tools (arrows, bezier arrows, callouts, pins, badges, step-arrows, labels, rects, rounded rects, circles, bullseyes, dividers, spotlights) that can be clicked to activate and placed onto the canvas.

#### Scenario: Selecting and placing a mark on canvas
- **WHEN** user selects an annotation tool from the toolbar and clicks or drags on the canvas
- **THEN** an annotation of that type is created and displayed as selected with visible interactive manipulation handles

### Requirement: Direct Manipulation of Arrow Endpoints and Handles
The system SHALL display interactive control handles on selected elements, enabling users to click and drag the start point, end point, and control curves of arrows and bezier arrows directly on the canvas.

#### Scenario: Dragging arrow start and end points
- **WHEN** user selects an arrow on the canvas and drags its start or end handle to a new position
- **THEN** the arrow's start or end coordinates update dynamically in real time and the arrow head reorients accordingly

#### Scenario: Dragging bezier arrow curvature handle
- **WHEN** user drags the intermediate control point handle of a bezier arrow
- **THEN** the curvature of the bezier curve updates smoothly to follow the handle position

### Requirement: Typography and Style Customization
The system SHALL provide an inspector panel allowing users to edit text content, choose font families (including system fonts), adjust font sizes, toggle text boxes/outlines, and switch semantic themes (primary, secondary, warning, danger, info, step).

#### Scenario: Editing annotation text and font family
- **WHEN** user edits the text field and selects a font family in the inspector for a callout or label
- **THEN** the annotation on the canvas immediately reflects the updated text and typography

#### Scenario: Changing semantic style
- **WHEN** user switches the style palette from `primary` to `warning`
- **THEN** the selected annotation changes its fill, stroke, and accent colors according to the MarkIts design token mapping

### Requirement: Edit History and Undo/Redo
The system SHALL maintain an undo/redo stack of canvas actions including adding, moving, editing handles, changing styles, and deleting annotations.

#### Scenario: Undo an edit action
- **WHEN** user presses Ctrl+Z (or Cmd+Z) after modifying an annotation position
- **THEN** the annotation reverts to its state prior to that modification
