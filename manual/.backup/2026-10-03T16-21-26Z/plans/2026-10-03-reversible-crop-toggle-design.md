# Reversible Cropping & Toggleable Mark-Crop Design

## Overview
This design adds reversible cropping capabilities to the MarkIts Desktop editor (`apps/desktop`).
Specifically:
1. Make "Crop to Marks (マークでクロップ)" a toggle button that crops to the annotation bounding box when activated, and restores the full original uncropped image and shifts all annotations back to original coordinates when deactivated.
2. Enable full reversibility for all crops (including manual crops) via Ctrl+Z (Undo) and a dedicated "元の画像に戻す (Revert Crop)" toolbar button.

---

## 1. State Management

In `AnnotationEditor` (`apps/desktop/src/editor.ts`):
- `baseImageState`: Keeps the pristine original state of the loaded or captured image:
  ```typescript
  interface BaseImageState {
    dataUrl: string;
    width: number;
    height: number;
    uiElements: DetectedUiElement[];
  }
  private baseImageState: BaseImageState | null = null;
  ```
- `isAutoCropped`: Boolean tracking whether auto-crop is currently active.
- `autoCropOffset`: Tracks `{ x: number, y: number }` applied during the active auto-crop.
- `cropHistoryStack`: Stack of pre-crop states so that every manual or auto-crop can be undone:
  ```typescript
  interface CropHistoryState {
    dataUrl: string;
    width: number;
    height: number;
    uiElements: DetectedUiElement[];
    offset: { x: number; y: number };
    sceneJson: string;
  }
  private cropHistoryStack: CropHistoryState[] = [];
  ```

---

## 2. Toggleable "🎯 マークでクロップ" Behavior

- **Button element**:
  `<button id="btn-autocrop" class="btn btn-toggle" title="マークに合わせてクロップ (トグル切替)">🎯 マークでクロップ</button>`
- **Method `toggleAutoCrop(margin = 32)`**:
  - **When toggling ON (`!this.isAutoCropped`)**:
    1. Guard against empty annotations: if `this.scene.annotations.length === 0`, do nothing.
    2. Compute annotation union bounding box + margin `(rx, ry, rw, rh)`.
    3. Save current state to `cropHistoryStack`.
    4. Render cropped canvas to PNG dataUrl.
    5. Shift annotations by `(-rx, -ry)` and UI elements by `(-rx, -ry)`.
    6. Store `this.autoCropOffset = { x: rx, y: ry }`.
    7. Update background image and set canvas size to `rw x rh`.
    8. Set `this.isAutoCropped = true`.
    9. Add `.active` class to `#btn-autocrop`.
  - **When toggling OFF (`this.isAutoCropped`)**:
    1. Restore background image to `baseImageState.dataUrl`.
    2. Restore canvas dimensions to `baseImageState.width, baseImageState.height`.
    3. Shift all current annotations (including any modified or newly added ones) back by `(+this.autoCropOffset.x, +this.autoCropOffset.y)`.
    4. Restore `uiElements` from `baseImageState.uiElements`.
    5. Set `this.isAutoCropped = false`.
    6. Remove `.active` class from `#btn-autocrop`.
    7. Trigger re-render and inspector update.

---

## 3. General Reversible Cropping

- **Undo (Ctrl+Z) Integration**:
  - `applyCrop()` no longer clears the undo stack (`undoStack = []`).
  - Instead, the previous crop state is saved to `cropHistoryStack`, and an undo state is pushed to `undoStack`.
  - When `undo()` is invoked and a crop is the top action, it restores the previous background image and canvas dimensions.
- **Revert Crop Action (`btn-revert-crop`)**:
  - A toolbar button "元の画像に戻す" is visible/enabled when the image is in a cropped state (`isCropped || isAutoCropped`).
  - Clicking it restores `baseImageState` and shifts annotations back to original image space.

---

## 4. Verification Plan

1. Desktop App Build: Run `npm run build` in `apps/desktop` to ensure TypeScript compilation and asset bundling succeed.
2. Workspace Tests: Run `cargo test --workspace` to ensure no backend regressions.
3. Behavior Verification:
   - Load image with annotations.
   - Click "🎯 マークでクロップ": Canvas shrinks to mark bounds, button turns active.
   - Click "🎯 マークでクロップ" again: Canvas and background restore to original full dimensions, annotations return to original positions, button turns inactive.
   - Manual crop: Apply crop, press Ctrl+Z (or click Revert), verify full image is restored.
