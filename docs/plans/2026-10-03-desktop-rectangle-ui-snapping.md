# Desktop Rectangle UI Snapping Implementation Plan

> **For Antigravity:** REQUIRED WORKFLOW: Use `.agent/workflows/execute-plan.md` to execute this plan in single-flow mode.

**Goal:** Enable snapping to UI element bounding boxes when dragging to create or move rectangular annotations in the MarkIts desktop editor.

**Architecture:** Implement `findSnapElementForRect` and `findSnapElementForMove` in `apps/desktop/src/editor.ts`. Integrate with `handleMouseMove` during annotation creation (`isCreating`) and annotation repositioning (`isDraggingAnnotation`), rendering visual active snap guides and preserving Alt-key bypass.

**Tech Stack:** TypeScript, Tauri 2, SVG, DOM APIs.

---

### Task 1: Implement `findSnapElementForRect` and `findSnapElementForMove` in `editor.ts`

**Files:**
- Modify: `apps/desktop/src/editor.ts:655-715`

**Step 1: Implement `findSnapElementForRect`**
Search `this.uiElements` for matching elements based on:
1. Overlap of dragged rect `[minX, minY, w, h]` with `el`.
2. Proximity of dragged corners/edges to `el`'s bounds within a generous threshold (24px).
3. Filter out huge background window bounds (> 65% canvas) when smaller child elements match.

**Step 2: Implement `findSnapElementForMove`**
For moving an existing annotation, check if `(x, y)` or center `(x + w/2, y + h/2)` is close to any `el`.

**Step 3: Verification**
Run: `npm run build` in `apps/desktop` to ensure TypeScript compiles without type errors.

**Step 4: Commit**
```bash
git add apps/desktop/src/editor.ts
git commit -m "feat: add rectangle UI element snapping helpers to editor"
```

---

### Task 2: Integrate UI snapping when creating annotations by dragging

**Files:**
- Modify: `apps/desktop/src/editor.ts:990-1030`

**Step 1: Update `handleMouseMove` during `isCreating`**
When `isCreating` is true and `'target' in anno`:
- If `!e.altKey && this.isSnapEnabled`:
  - Call `findSnapElementForRect(this.createStartX, this.createStartY, endX, endY)`.
  - If snapped:
    - Snap `(anno as any).target = [Math.round(el.x), Math.round(el.y), Math.round(el.width), Math.round(el.height)]`.
    - Call `this.renderAllSnapTargets({ x: el.x, y: el.y, snapped: true, element: el })`.
  - Else:
    - Normal freeform drag with `this.renderAllSnapTargets(snap)`.

**Step 2: Verification**
Run: `npm run build` in `apps/desktop`.

**Step 3: Commit**
```bash
git add apps/desktop/src/editor.ts
git commit -m "feat: snap rectangles to UI elements when dragging to create"
```

---

### Task 3: Integrate UI snapping when moving existing annotations

**Files:**
- Modify: `apps/desktop/src/editor.ts:960-990`

**Step 1: Update `handleMouseMove` during `isDraggingAnnotation`**
When `isDraggingAnnotation` is true and `'target' in anno && Array.isArray((anno as any).target)`:
- If `!e.altKey && this.isSnapEnabled`:
  - Calculate candidate `newX = initTarget[0] + dx`, `newY = initTarget[1] + dy`.
  - Call `findSnapElementForMove(newX, newY, initTarget[2], initTarget[3])`.
  - If snapped:
    - Align `target[0] = Math.round(el.x), target[1] = Math.round(el.y)`.
    - If size is similar (within 30%), optionally fit `target[2] = el.width, target[3] = el.height`.
    - Call `this.renderAllSnapTargets({ x: el.x, y: el.y, snapped: true, element: el })`.
  - Else:
    - Standard movement with `this.renderAllSnapTargets()`.

**Step 2: Verification**
Run: `npm run build` in `apps/desktop`.

**Step 3: Commit**
```bash
git add apps/desktop/src/editor.ts
git commit -m "feat: snap annotations to UI elements when dragging to move"
```

---

### Task 4: Final verification and testing

**Files:**
- Check: `apps/desktop/src/editor.ts`
- Run: `npm run build` in `apps/desktop`
- Run: `cargo test --workspace`

**Step 1: Run desktop build**
Expected: Build succeeds with 0 errors.

**Step 2: Run all workspace tests**
Expected: All tests PASS.

**Step 3: Commit**
```bash
git add docs/plans/
git commit -m "docs: finalize desktop rectangle UI snapping plan"
```
