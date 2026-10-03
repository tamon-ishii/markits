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
  private dimSvgEl: SVGSVGElement;
  private dimPathEl: SVGPathElement;
  private selectionBoxEl: HTMLElement;
  private sizeBadgeEl: HTMLElement;
  private hoverBoxEl: HTMLElement;
  private hoverBadgeEl: HTMLElement;

  private isMouseDown: boolean = false;
  private hasDragged: boolean = false;
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
    this.dimSvgEl = document.getElementById('overlay-dim-svg') as unknown as SVGSVGElement;
    this.dimPathEl = document.getElementById('overlay-dim-path') as unknown as SVGPathElement;
    this.selectionBoxEl = document.getElementById('selection-box')!;
    this.sizeBadgeEl = document.getElementById('selection-size-badge')!;
    this.hoverBoxEl = document.getElementById('ui-element-hover-box')!;
    this.hoverBadgeEl = document.getElementById('ui-element-badge')!;

    this.bindEvents();
  }

  private bindEvents() {
    this.overlayEl.addEventListener('mousedown', (e) => this.handleMouseDown(e));
    window.addEventListener('mousemove', (e) => this.handleMouseMove(e));
    window.addEventListener('mouseup', (e) => this.handleMouseUp(e));

    // Fullscreen capture button at bottom toolbar
    document.getElementById('btn-capture-fullscreen')?.addEventListener('click', () => {
      this.hide();
      if (this.onCompleteCallback) {
        this.onCompleteCallback(null, this.uiElements);
      }
    });

    // Cancel button
    document.getElementById('btn-cancel-capture')?.addEventListener('click', () => {
      this.cancel();
    });
    this.overlayEl.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      this.cancel();
    });

    // Keyboard shortcuts
    window.addEventListener('keydown', (e) => {
      if (this.overlayEl.classList.contains('hidden')) return;

      if (e.key === 'Escape') {
        e.preventDefault();
        this.cancel();
      } else if (e.key === 'Enter') {
        if (this.hoveredElement && this.hoveredElement.width > 20 && this.hoveredElement.height > 20) {
          const rect: CropRect = {
            x: this.hoveredElement.x,
            y: this.hoveredElement.y,
            width: this.hoveredElement.width,
            height: this.hoveredElement.height,
          };
          this.hide();
          if (this.onCompleteCallback) {
            this.onCompleteCallback(rect, this.uiElements);
          }
        } else if (this.currentRect && this.currentRect.width > 10 && this.currentRect.height > 10) {
          const rect = { ...this.currentRect };
          this.hide();
          if (this.onCompleteCallback) {
            this.onCompleteCallback(rect, this.uiElements);
          }
        }
      }
    });

    // Also handle double-click as confirmation
    this.overlayEl.addEventListener('dblclick', (e) => {
      if ((e.target as HTMLElement).closest('.overlay-toolbar')) return;

      if (this.hoveredElement && this.hoveredElement.width > 20 && this.hoveredElement.height > 20) {
        const rect: CropRect = {
          x: this.hoveredElement.x,
          y: this.hoveredElement.y,
          width: this.hoveredElement.width,
          height: this.hoveredElement.height,
        };
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
    this.isMouseDown = false;
    this.hasDragged = false;

    this.selectionBoxEl.style.display = 'none';
    this.hoverBoxEl.style.display = 'none';
    this.overlayEl.style.opacity = '0';
    this.overlayEl.classList.remove('hidden');

    const onImageReady = () => {
      // Entire screen dimmed gray initially
      this.updateDimming(null);
      this.overlayEl.style.opacity = '1';
    };

    if (this.bgImgEl.complete && this.bgImgEl.naturalWidth > 0) {
      onImageReady();
    } else {
      this.bgImgEl.onload = onImageReady;
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
    this.isMouseDown = false;
    this.hasDragged = false;
    this.selectionBoxEl.style.display = 'none';
    this.hoverBoxEl.style.display = 'none';
    this.updateDimming(null);
  }

  /**
   * Updates the SVG dimming layer using evenodd fill rule.
   * When cutoutRect is provided, that rectangle is "punched out", revealing the underlying crisp image.
   */
  private updateDimming(cutoutRect: CropRect | null) {
    if (!this.dimSvgEl || !this.dimPathEl) return;
    const nw = this.bgImgEl.naturalWidth || window.innerWidth;
    const nh = this.bgImgEl.naturalHeight || window.innerHeight;
    this.dimSvgEl.setAttribute('viewBox', `0 0 ${nw} ${nh}`);

    if (!cutoutRect || cutoutRect.width <= 0 || cutoutRect.height <= 0) {
      // Full screen dark gray overlay
      this.dimPathEl.setAttribute('d', `M 0 0 H ${nw} V ${nh} H 0 Z`);
    } else {
      // Punch out the window / selection rectangle
      const x = Math.max(0, Math.round(cutoutRect.x));
      const y = Math.max(0, Math.round(cutoutRect.y));
      const w = Math.min(nw - x, Math.round(cutoutRect.width));
      const h = Math.min(nh - y, Math.round(cutoutRect.height));
      this.dimPathEl.setAttribute(
        'd',
        `M 0 0 H ${nw} V ${nh} H 0 Z M ${x} ${y} H ${x + w} V ${y + h} H ${x} Z`
      );
    }
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
   * Find the window directly under the cursor.
   * Scans in reverse order so the topmost window according to Z-order stacking is chosen.
   */
  private findWindowAt(imgX: number, imgY: number): DetectedUiElement | null {
    // 1. Look for explicit role === 'window' matching cursor
    for (let i = this.uiElements.length - 1; i >= 0; i--) {
      const el = this.uiElements[i];
      if (
        el.role === 'window' &&
        el.width >= 40 &&
        el.height >= 40 &&
        imgX >= el.x &&
        imgX <= el.x + el.width &&
        imgY >= el.y &&
        imgY <= el.y + el.height
      ) {
        return el;
      }
    }

    // 2. Fallback to any container element
    for (let i = this.uiElements.length - 1; i >= 0; i--) {
      const el = this.uiElements[i];
      if (
        el.width >= 100 &&
        el.height >= 60 &&
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
    if (this.overlayEl.classList.contains('hidden')) return;
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest('.overlay-toolbar')) return;

    this.isMouseDown = true;
    this.hasDragged = false;
    this.startScreenX = e.clientX;
    this.startScreenY = e.clientY;

    const imgPt = this.screenToImage(e.clientX, e.clientY);
    this.startImgX = imgPt.x;
    this.startImgY = imgPt.y;

    this.currentRect = { x: this.startImgX, y: this.startImgY, width: 0, height: 0 };
  }

  private handleMouseMove(e: MouseEvent) {
    if (this.overlayEl.classList.contains('hidden')) return;

    const imgPt = this.screenToImage(e.clientX, e.clientY);

    if (this.isMouseDown) {
      const moveDist = Math.hypot(e.clientX - this.startScreenX, e.clientY - this.startScreenY);
      if (moveDist > 6) {
        this.hasDragged = true;
      }

      if (this.hasDragged) {
        // Freeform drag rectangle selection: hole-punch selection so user clearly sees what is selected
        const imgX = Math.min(this.startImgX, imgPt.x);
        const imgY = Math.min(this.startImgY, imgPt.y);
        const width = Math.abs(imgPt.x - this.startImgX);
        const height = Math.abs(imgPt.y - this.startImgY);

        this.currentRect = { x: imgX, y: imgY, width, height };
        this.updateBox();
        this.updateDimming(this.currentRect);

        this.selectionBoxEl.style.display = 'block';
        this.hoverBoxEl.style.display = 'none';
        return;
      }
    }

    // Hover mode: identify window under cursor and brighten it
    const win = this.findWindowAt(imgPt.x, imgPt.y);
    this.hoveredElement = win;

    if (win) {
      // Punch out window area so it becomes bright
      this.updateDimming({
        x: win.x,
        y: win.y,
        width: win.width,
        height: win.height,
      });

      const { scaleX, scaleY } = this.getScale();
      const sx = win.x / scaleX;
      const sy = win.y / scaleY;
      const sw = win.width / scaleX;
      const sh = win.height / scaleY;

      this.hoverBoxEl.style.left = `${sx}px`;
      this.hoverBoxEl.style.top = `${sy}px`;
      this.hoverBoxEl.style.width = `${sw}px`;
      this.hoverBoxEl.style.height = `${sh}px`;
      this.hoverBoxEl.style.display = 'block';

      const title = win.name ? `🪟 ${win.name}` : `🪟 ウィンドウ (${Math.round(win.width)} × ${Math.round(win.height)})`;
      this.hoverBadgeEl.textContent = title;
    } else {
      // No window under cursor: entire screen dimmed
      this.updateDimming(null);
      this.hoverBoxEl.style.display = 'none';
    }
  }

  private handleMouseUp(e: MouseEvent) {
    if (!this.isMouseDown) return;
    this.isMouseDown = false;

    if ((e.target as HTMLElement).closest('.overlay-toolbar')) return;

    if (!this.hasDragged) {
      // Single Click: capture the window directly!
      const imgPt = this.screenToImage(e.clientX, e.clientY);
      const win = this.hoveredElement || this.findWindowAt(imgPt.x, imgPt.y);

      if (win && win.width >= 30 && win.height >= 30) {
        const rect: CropRect = {
          x: win.x,
          y: win.y,
          width: win.width,
          height: win.height,
        };
        this.hide();
        if (this.onCompleteCallback) {
          this.onCompleteCallback(rect, this.uiElements);
        }
        return;
      }
    } else {
      // Drag selection completed: crop rectangle
      if (this.currentRect && this.currentRect.width >= 15 && this.currentRect.height >= 15) {
        const rect = { ...this.currentRect };
        this.hide();
        if (this.onCompleteCallback) {
          this.onCompleteCallback(rect, this.uiElements);
        }
        return;
      }
    }

    this.selectionBoxEl.style.display = 'none';
    this.updateDimming(null);
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
