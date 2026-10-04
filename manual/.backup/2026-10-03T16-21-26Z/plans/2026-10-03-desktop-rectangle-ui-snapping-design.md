# Design: Rectangle UI Element Snapping during Drag in Desktop Editor

**Date**: 2026-10-03  
**Status**: Approved  

---

## 1. Context & Motivation

The MarkIts desktop editor (`apps/desktop/src/editor.ts`) detects and displays UIMap elements and provides snapping.
Previously:
- Point-based snapping (`findSnapPoint`) snapped the cursor (or arrow heads) to 9 discrete points (corners, edge centers, center) of UI elements.
- When drawing a rectangle (`rect`, `rounded-rect`, `spotlight`, `circle`, `callout`), dragging mouse coordinates immediately overrode any UI element target with raw cursor coordinates `[min(x1, x2), min(y1, y2), abs(w), abs(h)]`.
- When moving an existing annotation (`isDraggingAnnotation`), there was no snapping at all.

Users expect that dragging a rectangle near or over a UI element will smoothly snap the rectangle's boundary to match the UI element's bounding box `[el.x, el.y, el.width, el.height]`.

---

## 2. Technical Design

### 2.1 Snapping Functions in `apps/desktop/src/editor.ts`

1. **`findSnapElementForRect(startX, startY, curX, curY, threshold)`**:
   - Calculates bounding box of current drag: `x, y, w, h`.
   - Compares with `uiElements`:
     - If the drag rectangle encompasses or substantially overlaps an element (`IoU` or overlap ratio >= 0.4, or corners within threshold).
     - Prioritizes smaller, specific child elements (like buttons) over large window backgrounds.
     - Returns `{ snapped: true, element: el, rect: [el.x, el.y, el.width, el.height] }`.
2. **`findSnapElementForMove(x, y, w, h, threshold)`**:
   - For moving existing annotations: checks if the annotation's top-left, center, or edges are within `threshold` of any UI element.
   - If matched, snaps position to align with `el`.

### 2.2 Integration into Event Handlers

1. **`handleMouseMove` when creating (`isCreating`)**:
   - For annotations with `target` (`rect`, `rounded-rect`, `spotlight`, `circle`, `callout`):
     - Calls `findSnapElementForRect`.
     - If snapped, sets `target = [el.x, el.y, el.width, el.height]` and highlights `el` in `activeSnapLayerEl`.
     - If not snapped, falls back to freeform drag with corner point snap.
2. **`handleMouseMove` when moving (`isDraggingAnnotation`)**:
   - For annotations with `target`:
     - Checks `findSnapElementForMove`.
     - If snapped, aligns with the element and highlights it.
3. **Bypass with Alt Key**:
   - Holding `Alt` bypasses snapping and enables 1px precision freeform dragging.

---

## 3. Verification

1. Build desktop app bundle: `npm run build` in `apps/desktop`.
2. Unit tests / TypeScript compilation: verify `tsc --noEmit`.
3. Test snapping interactively or via automated unit tests.
