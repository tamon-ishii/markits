import { DetectedUiElement } from './types.ts';

export interface CropRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export class CaptureOverlay {
  private overlayEl: HTMLElement;
  private bgImgEl: HTMLImageElement;
  private selectionBoxEl: HTMLElement;
  private sizeBadgeEl: HTMLElement;
  private hoverBoxEl: HTMLElement;
  private hoverBadgeEl: HTMLElement;
  private snapPointEl: HTMLElement;
  private uiElementsLayerEl: SVGSVGElement;

  private isSelecting: boolean = false;
  private startScreenX: number = 0;
  private startScreenY: number = 0;
  private startImgX: number = 0;
  private startImgY: number = 0;
  private currentRect: CropRect | null = null;
  private uiElements: DetectedUiElement[] = [];
  private hoveredElement: DetectedUiElement | null = null;
  private onCompleteCallback?: (rect: CropRect | null, elements: DetectedUiElement[]) => void;
  private onCancelCallback?: () => void;

  constructor() {
    this.overlayEl = document.getElementById('capture-overlay')!;
    this.bgImgEl = document.getElementById('overlay-bg') as HTMLImageElement;
    this.selectionBoxEl = document.getElementById('selection-box')!;
    this.sizeBadgeEl = document.getElementById('selection-size-badge')!;
    this.hoverBoxEl = document.getElementById('ui-element-hover-box')!;
    this.hoverBadgeEl = document.getElementById('ui-element-badge')!;
    this.snapPointEl = document.getElementById('snap-point-indicator')!;
    this.uiElementsLayerEl = document.getElementById('overlay-ui-elements-layer') as unknown as SVGSVGElement;

    this.bindEvents();
  }

  private bindEvents() {
    this.overlayEl.addEventListener('mousedown', (e) => this.handleMouseDown(e));
    window.addEventListener('mousemove', (e) => this.handleMouseMove(e));
    window.addEventListener('mouseup', (e) => this.handleMouseUp(e));

    document.getElementById('btn-capture-fullscreen')?.addEventListener('click', () => {
      if (this.onCompleteCallback) {
        this.onCompleteCallback(null, this.uiElements);
      }
      this.hide();
    });

    document.getElementById('btn-cancel-capture')?.addEventListener('click', () => {
      this.cancel();
    });

    window.addEventListener('keydown', (e) => {
      if (this.overlayEl.classList.contains('hidden')) return;

      if (e.key === 'Escape') {
        this.cancel();
      } else if (e.key === 'Enter') {
        if (this.currentRect && this.currentRect.width > 5 && this.currentRect.height > 5) {
          const rect = { ...this.currentRect };
          this.hide();
          if (this.onCompleteCallback) {
            this.onCompleteCallback(rect, this.uiElements);
          }
        }
      }
    });

    // Double click to confirm current selection
    this.overlayEl.addEventListener('dblclick', () => {
      if (this.currentRect && this.currentRect.width > 5 && this.currentRect.height > 5) {
        const rect = { ...this.currentRect };
        this.hide();
        if (this.onCompleteCallback) {
          this.onCompleteCallback(rect, this.uiElements);
        }
      }
    });
  }

  public show(
    fullDataUrl: string,
    uiElements: DetectedUiElement[] = [],
    onComplete: (rect: CropRect | null, elements: DetectedUiElement[]) => void,
    onCancel?: () => void
  ) {
    this.bgImgEl.src = fullDataUrl;
    this.uiElements = uiElements;
    this.onCompleteCallback = onComplete;
    this.onCancelCallback = onCancel;
    this.currentRect = null;
    this.hoveredElement = null;

    this.selectionBoxEl.style.display = 'none';
    this.hoverBoxEl.style.display = 'none';
    this.snapPointEl.style.display = 'none';
    this.overlayEl.classList.remove('hidden');

    if (this.bgImgEl.complete && this.bgImgEl.naturalWidth > 0) {
      this.renderUiElementRects();
    } else {
      this.bgImgEl.onload = () => {
        this.renderUiElementRects();
      };
    }
  }

  public cancel() {
    this.hide();
    if (this.onCancelCallback) {
      this.onCancelCallback();
    }
  }

  public hide() {
    this.overlayEl.classList.add('hidden');
    this.isSelecting = false;
    this.selectionBoxEl.style.display = 'none';
    this.hoverBoxEl.style.display = 'none';
    this.snapPointEl.style.display = 'none';
    if (this.uiElementsLayerEl) {
      this.uiElementsLayerEl.innerHTML = '';
    }
  }

  /**
   * Draw rectangles around all detectable UI elements so user immediately sees snappable targets.
   */
  private renderUiElementRects() {
    if (!this.uiElementsLayerEl) return;
    const nw = this.bgImgEl.naturalWidth || window.innerWidth;
    const nh = this.bgImgEl.naturalHeight || window.innerHeight;
    this.uiElementsLayerEl.setAttribute('viewBox', `0 0 ${nw} ${nh}`);

    const rectsHtml = this.uiElements
      .map((el, i) => {
        return `
          <g class="snap-target-group" data-index="${i}">
            <rect
              class="snap-target-rect"
              data-index="${i}"
              x="${el.x}"
              y="${el.y}"
              width="${el.width}"
              height="${el.height}"
            />
          </g>
        `;
      })
      .join('');

    this.uiElementsLayerEl.innerHTML = rectsHtml;
  }

  private setActiveRect(activeEl: DetectedUiElement | null) {
    if (!this.uiElementsLayerEl) return;
    const rects = this.uiElementsLayerEl.querySelectorAll('.snap-target-rect');
    rects.forEach((rect, i) => {
      const el = this.uiElements[i];
      if (activeEl && el === activeEl) {
        rect.classList.add('active');
      } else {
        rect.classList.remove('active');
      }
    });
  }

  private getScale(): { scaleX: number; scaleY: number } {
    const nw = this.bgImgEl.naturalWidth || window.innerWidth;
    const nh = this.bgImgEl.naturalHeight || window.innerHeight;
    const cw = window.innerWidth;
    const ch = window.innerHeight;
    return {
      scaleX: nw / cw,
      scaleY: nh / ch,
    };
  }

  private screenToImage(sx: number, sy: number): { x: number; y: number } {
    const { scaleX, scaleY } = this.getScale();
    return { x: sx * scaleX, y: sy * scaleY };
  }

  private imageToScreen(ix: number, iy: number): { x: number; y: number } {
    const { scaleX, scaleY } = this.getScale();
    return { x: ix / scaleX, y: iy / scaleY };
  }

  /**
   * Check if a screen point is close to any UI element corners, edges, or center,
   * or inside an element.
   */
  private findSnapPoint(screenX: number, screenY: number, thresholdPx: number = 14): {
    screenX: number;
    screenY: number;
    imgX: number;
    imgY: number;
    element: DetectedUiElement | null;
  } {
    const { scaleX, scaleY } = this.getScale();
    let bestDist = thresholdPx;
    let bestSnap: { sx: number; sy: number; ix: number; iy: number; el: DetectedUiElement } | null = null;

    for (const el of this.uiElements) {
      // Convert element bounds to screen coordinates
      const elSx = el.x / scaleX;
      const elSy = el.y / scaleY;
      const elSw = el.width / scaleX;
      const elSh = el.height / scaleY;

      // Key snap points
      const points = [
        // 4 corners
        { sx: elSx, sy: elSy, ix: el.x, iy: el.y },
        { sx: elSx + elSw, sy: elSy, ix: el.x + el.width, iy: el.y },
        { sx: elSx, sy: elSy + elSh, ix: el.x, iy: el.y + el.height },
        { sx: elSx + elSw, sy: elSy + elSh, ix: el.x + el.width, iy: el.y + el.height },
        // Edges center
        { sx: elSx + elSw / 2, sy: elSy, ix: el.x + el.width / 2, iy: el.y },
        { sx: elSx + elSw / 2, sy: elSy + elSh, ix: el.x + el.width / 2, iy: el.y + el.height },
        { sx: elSx, sy: elSy + elSh / 2, ix: el.x, iy: el.y + el.height / 2 },
        { sx: elSx + elSw, sy: elSy + elSh / 2, ix: el.x + el.width, iy: el.y + el.height / 2 },
      ];

      for (const pt of points) {
        const dist = Math.hypot(screenX - pt.sx, screenY - pt.sy);
        if (dist < bestDist) {
          bestDist = dist;
          bestSnap = { ...pt, el };
        }
      }
    }

    if (bestSnap) {
      return {
        screenX: bestSnap.sx,
        screenY: bestSnap.sy,
        imgX: bestSnap.ix,
        imgY: bestSnap.iy,
        element: bestSnap.el,
      };
    }

    // No snap point within threshold, return original point
    const imgPt = this.screenToImage(screenX, screenY);
    return {
      screenX,
      screenY,
      imgX: imgPt.x,
      imgY: imgPt.y,
      element: null,
    };
  }

  /**
   * Find the UI element directly under the cursor.
   */
  private findElementAt(imgX: number, imgY: number): DetectedUiElement | null {
    // Search in reverse so top-level/child elements match first
    for (let i = this.uiElements.length - 1; i >= 0; i--) {
      const el = this.uiElements[i];
      if (
        imgX >= el.x &&
        imgX <= el.x + el.width &&
        imgY >= el.y &&
        imgY <= el.y + el.height
      ) {
        return el;
      }
    }
    return null;
  }

  private handleMouseDown(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('.overlay-toolbar')) return;

    const snap = this.findSnapPoint(e.clientX, e.clientY);
    this.isSelecting = true;
    this.startScreenX = e.clientX;
    this.startScreenY = e.clientY;
    this.startImgX = snap.imgX;
    this.startImgY = snap.imgY;

    this.currentRect = { x: this.startImgX, y: this.startImgY, width: 0, height: 0 };
    this.updateBox();
    this.selectionBoxEl.style.display = 'block';
    this.hoverBoxEl.style.display = 'none';
  }

  private handleMouseMove(e: MouseEvent) {
    if (this.isSelecting) {
      // Dragging selection with snapping
      const snap = this.findSnapPoint(e.clientX, e.clientY);

      if (snap.element) {
        this.snapPointEl.style.left = `${snap.screenX}px`;
        this.snapPointEl.style.top = `${snap.screenY}px`;
        this.snapPointEl.style.display = 'block';
      } else {
        this.snapPointEl.style.display = 'none';
      }
      this.setActiveRect(snap.element);

      const imgX = Math.min(this.startImgX, snap.imgX);
      const imgY = Math.min(this.startImgY, snap.imgY);
      const width = Math.abs(snap.imgX - this.startImgX);
      const height = Math.abs(snap.imgY - this.startImgY);

      this.currentRect = { x: imgX, y: imgY, width, height };
      this.updateBox();
    } else {
      // Hovering mode: check if mouse is on a UI element or near a snap point
      const imgPt = this.screenToImage(e.clientX, e.clientY);
      const el = this.findElementAt(imgPt.x, imgPt.y);
      this.hoveredElement = el;

      if (el) {
        const { scaleX, scaleY } = this.getScale();
        const sx = el.x / scaleX;
        const sy = el.y / scaleY;
        const sw = el.width / scaleX;
        const sh = el.height / scaleY;

        this.hoverBoxEl.style.left = `${sx}px`;
        this.hoverBoxEl.style.top = `${sy}px`;
        this.hoverBoxEl.style.width = `${sw}px`;
        this.hoverBoxEl.style.height = `${sh}px`;
        this.hoverBoxEl.style.display = 'block';

        const label = el.name ? `[${el.role}] ${el.name}` : `[${el.role}]`;
        this.hoverBadgeEl.textContent = label;
      } else {
        this.hoverBoxEl.style.display = 'none';
      }

      // Snap point indicator when close to corner/edge
      const snap = this.findSnapPoint(e.clientX, e.clientY, 12);
      this.setActiveRect(snap.element || el);

      if (snap.element) {
        this.snapPointEl.style.left = `${snap.screenX}px`;
        this.snapPointEl.style.top = `${snap.screenY}px`;
        this.snapPointEl.style.display = 'block';
      } else {
        this.snapPointEl.style.display = 'none';
      }
    }
  }

  private handleMouseUp(e: MouseEvent) {
    if (!this.isSelecting) return;
    this.isSelecting = false;
    this.snapPointEl.style.display = 'none';
    this.setActiveRect(null);

    const screenMoveDist = Math.hypot(e.clientX - this.startScreenX, e.clientY - this.startScreenY);

    if (screenMoveDist < 6) {
      // User clicked without dragging: check if clicked on a UI element
      const imgPt = this.screenToImage(e.clientX, e.clientY);
      const el = this.hoveredElement || this.findElementAt(imgPt.x, imgPt.y);

      if (el) {
        // Select the clicked UI element directly!
        this.currentRect = {
          x: el.x,
          y: el.y,
          width: el.width,
          height: el.height,
        };
        this.updateBox();
        this.selectionBoxEl.style.display = 'block';
        return;
      }
    }

    if (this.currentRect && this.currentRect.width > 10 && this.currentRect.height > 10) {
      const rect = { ...this.currentRect };
      this.hide();
      if (this.onCompleteCallback) {
        this.onCompleteCallback(rect, this.uiElements);
      }
    } else {
      this.selectionBoxEl.style.display = 'none';
    }
  }

  private updateBox() {
    if (!this.currentRect) return;

    const topLeft = this.imageToScreen(this.currentRect.x, this.currentRect.y);
    const { scaleX, scaleY } = this.getScale();
    const sw = this.currentRect.width / scaleX;
    const sh = this.currentRect.height / scaleY;

    this.selectionBoxEl.style.left = `${topLeft.x}px`;
    this.selectionBoxEl.style.top = `${topLeft.y}px`;
    this.selectionBoxEl.style.width = `${sw}px`;
    this.selectionBoxEl.style.height = `${sh}px`;

    this.sizeBadgeEl.textContent = `${Math.round(this.currentRect.width)} × ${Math.round(this.currentRect.height)}`;
  }
}

