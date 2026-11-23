# annotation-editor Specification

## Purpose

Provides an interactive graphical canvas editor for placing marks, manipulating handles, snapping to UI elements, cropping, customizing styles and typography, and exporting annotated images.

## Requirements

### Requirement: Interactive Mark Tool Palette
The system SHALL provide a toolbar offering `select` plus primary annotation tools (`arrow`, `bezier-arrow`, `callout`, `label`, `badge`, `rect`, `circle`, `mosaic`, `spotlight`) that can be activated and placed onto the canvas, while also rendering and allowing inspection of additional model types (`rounded-rect`, `pin`, `step-arrow`, `bullseye`, `divider`) loaded from metadata.

#### Scenario: Selecting and placing a mark on canvas
- **WHEN** user selects an annotation tool from the toolbar and clicks or drags on the canvas
- **THEN** an annotation of that type is created and displayed as selected with visible manipulation handles

### Requirement: Direct Manipulation of Handles and Shapes
The system SHALL display interactive control handles on selected annotations, enabling users to drag arrow start/end points, Bezier curvature control points, and shape bounding box corners/positions directly on the canvas.

#### Scenario: Dragging arrow start and end points
- **WHEN** user selects an arrow and drags its start or end handle
- **THEN** the coordinates update in real time and the arrowhead reorients accordingly

#### Scenario: Dragging bezier arrow curvature handle
- **WHEN** user drags the intermediate control point handle of a bezier arrow
- **THEN** the curve updates smoothly to follow the handle position

### Requirement: Inspector Styling and Typography
The system SHALL provide an inspector for single or multi-selected marks supporting all seven semantic styles (`primary`, `secondary`, `warning`, `danger`, `info`, `step`, `pink`), stroke width, shadow, outline, shape line/fill/corner styles, mosaic block size, text content, font family, and font size.

#### Scenario: Bulk editing multiple selected annotations
- **WHEN** user selects multiple marks and changes a shared property in the inspector
- **THEN** all selected compatible annotations update together and re-render immediately

### Requirement: Arrow Skins and Label Placement
For `arrow` and `bezier-arrow` marks, the inspector SHALL offer three arrow skins with previews (`classic`, `sketch`, `bold`), classic-only line pattern (`solid`, `dashed`, `dotted`) and arrowhead (`filled`, `open`) controls, and `text_placement` (`middle` vs `end` at the tail) serialized only as canonical `text_placement`.

#### Scenario: Switching arrow skin and text placement
- **WHEN** user selects an arrow, picks the `sketch` or `bold` skin, or switches label placement between `middle` and `end`
- **THEN** the canvas updates immediately, classic-only line/arrowhead controls disable for non-classic skins, and scene JSON serializes only canonical `text_placement`

### Requirement: In-Editor Interactive Cropping
The system SHALL provide an interactive in-editor crop mode with resize handles, resolution/aspect presets, a "Fit to Marks" bounds action, reversible crop history, and automatic commitment of any pending crop before saving, copying, or finishing editing.

#### Scenario: Fit crop frame to marks and apply
- **WHEN** user enters crop mode, clicks "Fit to Marks", and applies the crop
- **THEN** the canvas crops to the annotation bounding box, translates annotation and UIMap coordinates, and preserves the pre-crop image for reversal

#### Scenario: Commit pending crop on save or copy
- **WHEN** user clicks Save, Copy, or Done Editing while crop mode has an unapplied selection
- **THEN** the pending crop is applied first before generating output

### Requirement: UI Element Snapping
The system SHALL provide toggleable UI element snapping (with Alt key temporary toggle) that highlights detected UI targets with visual guides while dragging or placing marks and reports asynchronous UI scan status.

#### Scenario: Snap rectangle to detected UI control
- **WHEN** snap is enabled and the user moves or draws a mark near a detected UI element
- **THEN** visual snap guides appear and the mark aligns to the UI element's bounds

### Requirement: Layer Visibility, Clipboard Marks, and Edit History
The system SHALL provide a mark layer list supporting visibility toggles (`hidden_annotations`), copy/paste of selected marks (`Ctrl+C`/`Ctrl+V`), and an undo/redo stack (`Ctrl+Z`/`Ctrl+Y`) covering additions, moves, handle edits, style changes, and deletions.

#### Scenario: Undo an edit action
- **WHEN** user presses Ctrl+Z (or Cmd+Z) after modifying an annotation
- **THEN** the annotation reverts to its state prior to that modification

#### Scenario: Toggle mark visibility in layer list
- **WHEN** user hides a mark from the layer list
- **THEN** its index is recorded in `hidden_annotations` and it is excluded from canvas rendering and exports without being deleted

### Requirement: Output Dimensions and Export Controls
The system SHALL provide output width/height inputs with an aspect-ratio lock, an export format selector (`PNG`, `WebP`, `JPEG XL`), an optional reproduction JSON export toggle, and (in external integration mode) an editable metadata retention toggle.

#### Scenario: Export at resized output dimensions
- **WHEN** user adjusts output width with aspect lock enabled and saves or copies the image
- **THEN** height updates proportionally and the exported image is rendered at the chosen dimensions
