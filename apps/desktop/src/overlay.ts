import { DetectedUiElement } from './types.ts';
import { findWindowAt } from './window-hit-test.ts';

export interface CropRect {
  x: number;
  y: number;
  width: number;
  height: number;
}
export type CursorCaptureSelection = { kind: 'window'; window: DetectedUiElement } | { kind: 'fullscreen' };

export class CaptureOverlay {
  private overlayEl: HTMLElement;
  private bgImgEl: HTMLImageElement;
  private dimSvgEl: SVGSVGElement;
  private dimPathEl: SVGPathElement;
  private hoverBoxEl: HTMLElement;
  private hoverBadgeEl: HTMLElement;

  private uiElements: DetectedUiElement[] = [];
  private hoveredElement: DetectedUiElement | null = null;
  private onCompleteCallback?: (rect: CropRect | null, elements: DetectedUiElement[]) => void;
  private onCancelCallback?: () => void;
  private onRecordCallback?: (window: DetectedUiElement, includeCursor: boolean) => void;
  private onCursorCallback?: (selection: CursorCaptureSelection) => void;
  private recordingMode = false;
  private includeCursor = false;
  private captureShortcut = 'PrintScreen';

  public setCursorSupported(supported: boolean) {
    const option = document.getElementById('capture-cursor-option');
    if (option) option.hidden = !supported;
  }

  private captureWindow(win: DetectedUiElement) {
    if (this.recordingMode) { this.recordWindow(win); return; }
    if (this.includeCursor) {
      this.hide();
      this.onCursorCallback?.({ kind: 'window', window: win });
      return;
    }
    this.hide();
    this.onCompleteCallback?.({ x: win.x, y: win.y, width: win.width, height: win.height }, this.uiElements);
  }

  private captureFullscreen() {
    this.hide();
    if (this.includeCursor) {
      this.onCursorCallback?.({ kind: 'fullscreen' });
    } else { this.onCompleteCallback?.(null, this.uiElements); }
  }

  private setRecordingMode(recording: boolean) {
    this.recordingMode = recording;
    this.overlayEl.style.cursor = 'default';
    for (const [id, active] of [['btn-mode-capture', !recording], ['btn-mode-record', recording]] as const) {
      const button = document.getElementById(id);
      button?.classList.toggle('btn-primary', active);
      button?.setAttribute('aria-pressed', String(active));
    }
    this.updateCaptureHint();
    (document.getElementById('btn-capture-fullscreen') as HTMLButtonElement).disabled = recording;
  }

  private setCursorIncluded(included: boolean) {
    this.includeCursor = included;
    (document.getElementById('capture-include-cursor') as HTMLInputElement).checked = included;
    this.updateCaptureHint();
  }

  private updateCaptureHint() {
    const key = this.captureShortcut.replace('Control', 'Ctrl');
    document.getElementById('ui-element-action-hint')!.textContent = this.recordingMode ? 'クリックで操作記録を開始' : this.includeCursor ? 'クリックで撮影対象を選択' : 'クリックで撮影';
    document.querySelector('.overlay-hint')!.textContent = this.recordingMode
      ? `記録対象をクリックしてください。記録中は${key}キーで撮影できます。`
      : this.includeCursor ? `対象を選択後、カーソルを動かし、${key}キーをもう一度押して撮影します。`
      : 'ウィンドウをクリックして撮影。範囲の調整は撮影後にクロップできます。';
  }

  private recordWindow(win: DetectedUiElement | null) {
    if (!win?.window_id) {
      alert('このウィンドウを特定できません。別のウィンドウを選択してください。');
      return;
    }
    this.hide();
    this.onRecordCallback?.(win, this.includeCursor);
  }

  constructor() {
    this.overlayEl = document.getElementById('capture-overlay')!;
    this.bgImgEl = document.getElementById('overlay-bg') as HTMLImageElement;
    this.dimSvgEl = document.getElementById('overlay-dim-svg') as unknown as SVGSVGElement;
    this.dimPathEl = document.getElementById('overlay-dim-path') as unknown as SVGPathElement;
    this.hoverBoxEl = document.getElementById('ui-element-hover-box')!;
    this.hoverBadgeEl = document.getElementById('ui-element-badge')!;

    this.bindEvents();
  }

  private bindEvents() {
    document.getElementById('btn-mode-capture')?.addEventListener('click', () => this.setRecordingMode(false));
    document.getElementById('btn-mode-record')?.addEventListener('click', () => this.setRecordingMode(true));
    document.getElementById('capture-include-cursor')?.addEventListener('change', event => this.setCursorIncluded((event.target as HTMLInputElement).checked));
    document.getElementById('btn-record-hovered-window')?.addEventListener('click', () => { this.setRecordingMode(true); this.recordWindow(this.hoveredElement); });
    this.overlayEl.addEventListener('click', (e) => this.handleWindowClick(e));
    window.addEventListener('mousemove', (e) => this.handleMouseMove(e));

    // Fullscreen capture button at bottom toolbar
    document.getElementById('btn-capture-fullscreen')?.addEventListener('click', () => {
      this.captureFullscreen();
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
        // Let focused buttons handle Enter without capturing the window first.
        if ((e.target as HTMLElement).closest('.overlay-toolbar, .ui-element-header')) return;
        if (this.recordingMode) { this.recordWindow(this.hoveredElement); return; }
        if (this.hoveredElement && this.hoveredElement.width > 20 && this.hoveredElement.height > 20) {
          this.captureWindow(this.hoveredElement);
        }
      }
    });

  }

  public show(
    fullDataUrl: string,
    uiElements: DetectedUiElement[] = [],
    onComplete: (rect: CropRect | null, elements: DetectedUiElement[]) => void,
    onCancel?: () => void,
    onRecord?: (window: DetectedUiElement, includeCursor: boolean) => void,
    onCursor?: (selection: CursorCaptureSelection) => void,
    recordingMode = false,
    captureShortcut = 'PrintScreen'
  ) {
    this.bgImgEl.src = fullDataUrl;
    this.uiElements = uiElements;
    this.onCompleteCallback = onComplete;
    this.onCancelCallback = onCancel;
    this.onRecordCallback = onRecord;
    this.onCursorCallback = onCursor;
    this.captureShortcut = captureShortcut;
    this.includeCursor = false;
    (document.getElementById('capture-include-cursor') as HTMLInputElement).checked = false;
    this.setRecordingMode(recordingMode);
    this.hoveredElement = null;

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
    this.overlayEl.style.cursor = 'default';
    this.overlayEl.classList.add('hidden');
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

  /**
   * Find the window directly under the cursor.
   * Scans in reverse order so the topmost window according to Z-order stacking is chosen.
   */
  private findWindowAt(imgX: number, imgY: number): DetectedUiElement | null {
    return findWindowAt(this.uiElements, imgX, imgY);
  }

  private handleMouseMove(e: MouseEvent) {
    if (this.overlayEl.classList.contains('hidden')) return;
    if ((e.target as HTMLElement).closest('.overlay-toolbar, .ui-element-header')) return;
    const imgPt = this.screenToImage(e.clientX, e.clientY);

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

  private handleWindowClick(e: MouseEvent) {
    if (this.overlayEl.classList.contains('hidden') || e.button !== 0) return;
    if ((e.target as HTMLElement).closest('.overlay-toolbar, .ui-element-header')) return;
    const point = this.screenToImage(e.clientX, e.clientY);
    const target = this.findWindowAt(point.x, point.y);
    if (target && target.width >= 30 && target.height >= 30) this.captureWindow(target);
  }
}
