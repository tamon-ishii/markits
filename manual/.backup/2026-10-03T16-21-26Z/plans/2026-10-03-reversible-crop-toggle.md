# Reversible Cropping & Toggleable Mark-Crop Implementation Plan

> **For Antigravity:** REQUIRED WORKFLOW: Use `.agent/workflows/execute-plan.md` to execute this plan in single-flow mode.

**Goal:** Implement reversible cropping (manual crop revert & undo support) and toggleable "Crop to Marks" in MarkIts Desktop editor so toggling off restores the original image and coordinates.

**Architecture:** Maintain a pristine `BaseImageState` and `CropHistoryState` stack in `AnnotationEditor`. Implement `toggleAutoCrop()` which crops to marks when active and restores original image and shifts annotation coordinates back when toggled off. Ensure `applyCrop()` pushes undoable history instead of wiping the undo stack, and provide a "元の画像に戻す" button.

**Tech Stack:** TypeScript, Vite, HTML5 Canvas, SVG.

---

### Task 1: Store `BaseImageState` and Preserve Crop History in `apps/desktop/src/editor.ts`

**Files:**
- Modify: `apps/desktop/src/editor.ts`

**Step 1: Define `BaseImageState` and `CropHistoryState` interfaces**
```typescript
interface BaseImageState {
  dataUrl: string;
  width: number;
  height: number;
  uiElements: DetectedUiElement[];
}

interface CropHistoryState {
  dataUrl: string;
  width: number;
  height: number;
  uiElements: DetectedUiElement[];
  offset: { x: number; y: number };
  annotations: any[];
}
```

**Step 2: Update `setBackgroundImage` and `AnnotationEditor` fields**
- Add:
  - `private baseImageState: BaseImageState | null = null;`
  - `private isAutoCropped: boolean = false;`
  - `private autoCropOffset: { x: number; y: number } = { x: 0, y: 0 };`
  - `private cropHistoryStack: CropHistoryState[] = [];`
- In `setBackgroundImage`:
  - If `isInternalCrop !== true`, record `baseImageState = { dataUrl, width, height, uiElements: [...this.uiElements] }`.

**Step 3: Verify build**
Run: `cd apps/desktop && npm run build`
Expected: PASS

---

### Task 2: Implement Toggleable Auto-Crop (`toggleAutoCrop`)

**Files:**
- Modify: `apps/desktop/src/editor.ts`
- Modify: `apps/desktop/src/main.ts`
- Modify: `apps/desktop/index.html`

**Step 1: Implement `toggleAutoCrop(margin = 32)` in `editor.ts`**
- If `!this.isAutoCropped`:
  - Calculate `getAnnotationBounds()`.
  - Push current state to `cropHistoryStack`.
  - Crop image via canvas and shift annotations by `(-rx, -ry)`.
  - Save `this.autoCropOffset = { x: rx, y: ry }`.
  - Set `this.isAutoCropped = true`.
  - Update `#btn-autocrop` styling (`classList.add('active')`).
- If `this.isAutoCropped`:
  - Restore background image to `baseImageState.dataUrl`.
  - Restore canvas size to `baseImageState.width, baseImageState.height`.
  - Shift all current annotations back by `(+this.autoCropOffset.x, +this.autoCropOffset.y)`.
  - Restore UI elements to `baseImageState.uiElements`.
  - Set `this.isAutoCropped = false`.
  - Update `#btn-autocrop` styling (`classList.remove('active')`).
  - Re-render.

**Step 2: Update `apps/desktop/src/main.ts` and `apps/desktop/index.html`**
- Set `#btn-autocrop` to call `editor.toggleAutoCrop()`.
- Add class `btn-toggle` to `#btn-autocrop` in `index.html`.

**Step 3: Verify build**
Run: `cd apps/desktop && npm run build`
Expected: PASS

---

### Task 3: Implement Revert Crop and Undo Support for Manual Crops

**Files:**
- Modify: `apps/desktop/src/editor.ts`
- Modify: `apps/desktop/index.html`

**Step 1: Implement `revertCrop()` in `editor.ts`**
- If cropped (`cropHistoryStack.length > 0` or canvas !== baseImageState):
  - Pop from `cropHistoryStack` (or revert directly to `baseImageState`).
  - Restore original background image and dimensions.
  - Shift annotations back by the cumulative crop offset.
  - Re-render.

**Step 2: Update `undo()` to handle crop reverts**
- If top of undo stack is a crop operation, call `revertCrop()` instead of failing.
- Add "元の画像に戻す" button `#btn-revert-crop` in `apps/desktop/index.html` next to `#btn-crop`.
- Wire click listener in `apps/desktop/src/main.ts`.

**Step 3: Verify build**
Run: `cd apps/desktop && npm run build`
Expected: PASS

---

### Task 4: Full Workspace Verification

**Files:**
- Verify: Full workspace tests and desktop build.

**Step 1: Run workspace tests**
Run: `cargo test --workspace`
Expected: PASS (93 tests).

**Step 2: Run desktop build**
Run: `cd apps/desktop && npm run build`
Expected: PASS.
