import { Annotation, BezierArrowAnnotation, ArrowAnnotation, Scene, SemanticStyle, PositionHint, DetectedUiElement } from './types.ts';

function escapeXml(str: string): string {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&apos;');
}


export interface BaseImageState {
  dataUrl: string;
  width: number;
  height: number;
  uiElements: DetectedUiElement[];
}

export interface CropHistoryState {
  dataUrl: string;
  width: number;
  height: number;
  uiElements: DetectedUiElement[];
  offset: { x: number; y: number };
  annotations: any[];
}

export class AnnotationEditor {
  private viewportEl: HTMLElement;
  private canvasContainerEl: HTMLElement;
  private bgImgEl: HTMLImageElement;
  private svgLayerEl: HTMLElement;
  private handlesLayerEl: SVGSVGElement;
  private snapGuideLayerEl: SVGSVGElement;
  private activeSnapLayerEl: SVGGElement | null = null;
  private inspectorEl: HTMLElement;
  private layerListEl: HTMLElement;
  private layerCountEl: HTMLElement;
  private emptyStateEl: HTMLElement;

  // In-Editor Crop Overlay elements
  private editorCropOverlayEl: HTMLElement | null = null;
  private cropBoxEl: HTMLElement | null = null;
  private cropSizeBadgeEl: HTMLElement | null = null;
  private cropMaskTop: HTMLElement | null = null;
  private cropMaskBottom: HTMLElement | null = null;
  private cropMaskLeft: HTMLElement | null = null;
  private cropMaskRight: HTMLElement | null = null;

  // Zoom elements & state
  private zoomLabelEl: HTMLElement | null = null;
  private zoomLevel: number = 1.0;
  private outputWidth: number = 800;
  private outputHeight: number = 600;

  // Crop mode state
  private isCropMode: boolean = false;
  private cropRect: { x: number; y: number; width: number; height: number } | null = null;
  private isDraggingCrop: boolean = false;
  private activeCropHandle: string | null = null;
  private cropDragStartX: number = 0;
  private cropDragStartY: number = 0;
  private initialCropRect: { x: number; y: number; width: number; height: number } | null = null;
  private cropPresetSelectEl: HTMLSelectElement | null = null;
  private cropPresetRatio: number | null = null;

  private scene: Scene = {
    canvas: { width: 800, height: 600 },
    shadow: true,
    annotations: [],
  };

  private uiElements: DetectedUiElement[] = [];
  private isSnapEnabled: boolean = true;

  private baseImageState: BaseImageState | null = null;
  private isAutoCropped: boolean = false;
  private autoCropOffset: { x: number; y: number } = { x: 0, y: 0 };
  private cropHistoryStack: CropHistoryState[] = [];

  private activeTool: string = 'select';
  private selection = new Set<number>();
  private primary: number | null = null;
  private get selectedIndex() { return this.primary; }
  private set selectedIndex(value: number | null) { this.primary = value; this.selection = new Set(value === null ? [] : [value]); }
  private marquee: {x:number;y:number;endX:number;endY:number;initial:number[]} | null = null;
  private selectMany(indices: number[]) {
    this.selectedIndex = indices[0] ?? null;
    this.selection = new Set(indices);
    this.updateHandles(); this.updateInspector();
  }
  private toggleSelection(index: number) {
    const selected = new Set(this.selection);
    if (selected.has(index)) selected.delete(index); else selected.add(index);
    this.selectMany([...selected]);
  }
  private bounds(index: number) {
    const a = this.scene.annotations[index] as any;
    const visibleIndices = this.scene.annotations.map((_, i) => i)
      .filter(i => !this.scene.hidden_annotations?.includes(i));
    // Badges and labels can be automatically placed outside their targets.
    for (const element of this.svgLayerEl.querySelectorAll<SVGGraphicsElement>('circle[data-badge-index], rect[data-label-index]')) {
      const renderedIndex = Number(element.dataset.labelIndex ?? element.dataset.badgeIndex);
      if (visibleIndices[renderedIndex] === index) {
        const box = element.getBBox();
        return { x: box.x, y: box.y, width: box.width, height: box.height };
      }
    }
    const points: number[][] = a.start && a.end ? [a.start, a.end, ...(a.control ? [a.control] : [])] : a.target ? [[a.target[0],a.target[1]],[a.target[0]+a.target[2],a.target[1]+a.target[3]]] : [];
    const x = Math.min(...points.map(p=>p[0])), y = Math.min(...points.map(p=>p[1]));
    return {x,y,width:Math.max(...points.map(p=>p[0]))-x,height:Math.max(...points.map(p=>p[1]))-y};
  }
  private selectionBox(x:number,y:number,width:number,height:number) {
    const box = document.createElementNS('http://www.w3.org/2000/svg','rect');
    for (const [key,value] of Object.entries({x,y,width,height})) box.setAttribute(key,String(value));
    box.setAttribute('fill','rgba(59,130,246,0.08)'); box.setAttribute('stroke','#3b82f6');
    box.setAttribute('stroke-dasharray','5 3'); box.setAttribute('pointer-events','none');
    this.handlesLayerEl.appendChild(box);
  }

  private undoStack: string[] = [];
  private redoStack: string[] = [];
  private initialSceneJson: string = '';
  private initialBgDataUrl: string = '';
  private initialIsAutoCropped: boolean = false;
  private initialUiElementsJson: string = '[]';

  // Dragging state
  private isDraggingHandle: boolean = false;
  private isDraggingAnnotation: boolean = false;
  private activeHandle: { type: string; index: number } | null = null;
  private dragStartX: number = 0;
  private dragStartY: number = 0;
  private initialAnnotationState: any = null;

  // New element drag state
  private isCreating: boolean = false;
  private createStartX: number = 0;
  private createStartY: number = 0;

  // Render throttling / RAF coalescing state
  private isRendering: boolean = false;
  private renderQueued: boolean = false;
  private rafId: number | null = null;

  private renderCallback: (sceneJson: string) => Promise<string>;

  constructor(renderCallback: (sceneJson: string) => Promise<string>) {
    this.renderCallback = renderCallback;
    this.viewportEl = document.getElementById('viewport')!;
    this.canvasContainerEl = document.getElementById('canvas-container')!;
    this.bgImgEl = document.getElementById('bg-image') as HTMLImageElement;
    this.svgLayerEl = document.getElementById('svg-layer')!;
    this.handlesLayerEl = document.getElementById('handles-layer') as unknown as SVGSVGElement;
    this.snapGuideLayerEl = document.getElementById('snap-guide-layer') as unknown as SVGSVGElement;
    this.inspectorEl = document.getElementById('inspector-content')!;
    this.layerListEl = document.getElementById('layer-list')!;
    this.layerCountEl = document.getElementById('layer-count')!;
    this.emptyStateEl = document.getElementById('empty-state')!;

    // Crop elements
    this.editorCropOverlayEl = document.getElementById('editor-crop-overlay');
    this.cropBoxEl = document.getElementById('crop-box');
    this.cropSizeBadgeEl = document.getElementById('crop-size-badge');
    this.cropMaskTop = document.getElementById('crop-mask-top');
    this.cropMaskBottom = document.getElementById('crop-mask-bottom');
    this.cropMaskLeft = document.getElementById('crop-mask-left');
    this.cropMaskRight = document.getElementById('crop-mask-right');
    this.cropPresetSelectEl = document.getElementById('crop-preset-select') as HTMLSelectElement | null;

    // Zoom elements
    this.zoomLabelEl = document.getElementById('zoom-label');

    this.bindEvents();
    this.layerListEl.addEventListener('click', (event) => {
      const button = (event.target as HTMLElement).closest<HTMLButtonElement>('button[data-layer-action]');
      if (!button) return;
      const index = Number(button.dataset.layerIndex);
      if (!Number.isInteger(index) || index < 0 || index >= this.scene.annotations.length) return;
      switch (button.dataset.layerAction) {
        case 'select':
          this.setTool('select');
          if ((event as MouseEvent).shiftKey) this.toggleSelection(index); else this.selectedIndex = index;
          this.updateHandles();
          this.updateInspector();
          break;
        case 'visibility':
          this.toggleLayerVisibility(index);
          break;
        case 'up':
          this.moveLayer(index, 1);
          break;
        case 'down':
          this.moveLayer(index, -1);
          break;
      }
    });
  }

  public setTool(tool: string) {
    this.activeTool = tool;
    document.querySelectorAll('.tool-btn').forEach((btn) => {
      if (btn.getAttribute('data-type') === tool) {
        btn.classList.add('active');
      } else {
        btn.classList.remove('active');
      }
    });

    this.clearSnapGuide();
    window.dispatchEvent(new CustomEvent('markits-tool-changed', { detail: tool }));
    if (tool !== 'select') {
      this.selectedIndex = null;
      this.updateHandles();
      this.updateInspector();
    }
  }

  public setBackgroundImage(
    dataUrl: string,
    width: number,
    height: number,
    annotationsJson?: string | null,
    uiElements?: DetectedUiElement[] | null,
    isInternalCrop: boolean = false
  ) {
    this.bgImgEl.src = dataUrl;
    window.dispatchEvent(new CustomEvent('markits-background-changed', {
      detail: isInternalCrop ? this.baseImageState?.dataUrl ?? dataUrl : dataUrl,
    }));
    this.scene.canvas = { width, height };
    this.outputWidth = width;
    this.outputHeight = height;
    this.setZoom(this.zoomLevel);
    this.uiElements = this.sanitizeUiElements(uiElements, width, height);
    this.activeSnapLayerEl = null;

    if (!isInternalCrop) {
      this.baseImageState = {
        dataUrl,
        width,
        height,
        uiElements: this.uiElements.map((el) => ({ ...el })),
      };
      this.isAutoCropped = false;
      this.autoCropOffset = { x: 0, y: 0 };
      this.cropHistoryStack = [];
      const btnAutoCrop = document.getElementById('btn-autocrop');
      if (btnAutoCrop) btnAutoCrop.classList.remove('active');
      this.updateRevertCropButton();
    }

    this.canvasContainerEl.style.width = `${width}px`;
    this.canvasContainerEl.style.height = `${height}px`;
    this.canvasContainerEl.style.display = 'block';
    this.emptyStateEl.style.display = 'none';

    this.handlesLayerEl.setAttribute('viewBox', `0 0 ${width} ${height}`);
    if (this.snapGuideLayerEl) {
      this.snapGuideLayerEl.setAttribute('viewBox', `0 0 ${width} ${height}`);
    }

    if (annotationsJson) {
      try {
        const parsed = JSON.parse(annotationsJson);
        this.scene = {
          canvas: { width, height },
          shadow: parsed.shadow ?? true,
          annotations: parsed.annotations ?? [],
          hidden_annotations: parsed.hidden_annotations ?? [],
        };
      } catch (e) {
        console.error('Failed to parse embedded annotations JSON', e);
        this.scene.annotations = [];
        this.scene.hidden_annotations = [];
      }
    } else {
      this.scene.annotations = [];
      this.scene.hidden_annotations = [];
    }

    if (!isInternalCrop) {
      this.undoStack = [];
      this.redoStack = [];
    }
    this.selectedIndex = null;
    this.pushState();
    if (!isInternalCrop) {
      this.initialSceneJson = this.getSceneJson();
      this.initialBgDataUrl = this.bgImgEl.src;
      this.initialIsAutoCropped = this.isAutoCropped;
      this.initialUiElementsJson = JSON.stringify(this.uiElements);
    }
    this.render();
    this.updateInspector();
    this.renderAllSnapTargets();
    if (!isInternalCrop) {
      setTimeout(() => this.zoomFit(), 50);
    }
    window.dispatchEvent(new CustomEvent('markits-image-loaded', { detail: { width, height } }));
  }

  public updateRevertCropButton(): void {
    const btnRevert = document.getElementById('btn-revert-crop') as HTMLButtonElement | null;
    if (btnRevert) {
      const isCropped = this.isAutoCropped || this.cropHistoryStack.length > 0;
      btnRevert.disabled = !isCropped;
      btnRevert.style.display = isCropped ? 'inline-flex' : 'none';
    }
  }

  public toggleSnap(): boolean {
    this.isSnapEnabled = !this.isSnapEnabled;
    this.renderAllSnapTargets();
    return this.isSnapEnabled;
  }

  public setSnapEnabled(enabled: boolean): void {
    this.isSnapEnabled = enabled;
    this.renderAllSnapTargets();
  }

  public isSnapActive(): boolean {
    return this.isSnapEnabled;
  }

  public setUiElements(elements: DetectedUiElement[]): void {
    const width = this.scene.canvas.width;
    const height = this.scene.canvas.height;
    this.uiElements = this.sanitizeUiElements(elements, width, height);
    this.activeSnapLayerEl = null;
    console.log(`[MarkIts Editor] Loaded ${this.uiElements.length} snappable UI elements`);
    this.renderAllSnapTargets();
  }

  private sanitizeUiElements(
    elements: DetectedUiElement[] | null | undefined,
    width: number,
    height: number
  ): DetectedUiElement[] {
    if (!elements || elements.length === 0) return [];
    // Only ignore purely transient menus / toolbars
    const ignoredRoles = new Set(['menubar', 'menu', 'viewport']);
    return elements.filter((el) => {
      if (ignoredRoles.has(el.role)) return false;
      // Must be within canvas
      if (el.x < 0 || el.y < 0 || el.x >= width || el.y >= height) return false;
      // Filter out tiny artifacts
      if (el.width < 6 || el.height < 6) return false;
      // Filter ghost / unmapped elements that default to absolute (0, 0)
      if (el.x === 0 && el.y === 0 && (el.role === 'menuitem' || !el.name)) return false;
      // A captured window can nearly fill the image; only discard its exact canvas border.
      if (Math.abs(el.x) <= 4 && Math.abs(el.y) <= 4 &&
          Math.abs(el.width - width) <= 8 && Math.abs(el.height - height) <= 8) {
        return false;
      }
      return true;
    });
  }

  public getUiElements(): DetectedUiElement[] {
    return [...this.uiElements];
  }

  public isDirty(): boolean {
    return (
      this.getSceneJson() !== this.initialSceneJson ||
      this.bgImgEl.src !== this.initialBgDataUrl ||
      this.isAutoCropped !== this.initialIsAutoCropped ||
      JSON.stringify(this.uiElements) !== this.initialUiElementsJson
    );
  }

  public markClean(): void {
    this.initialSceneJson = this.getSceneJson();
    this.initialBgDataUrl = this.bgImgEl.src;
    this.initialIsAutoCropped = this.isAutoCropped;
    this.initialUiElementsJson = JSON.stringify(this.uiElements);
  }

  public getCropState(): {
    isAutoCropped: boolean;
    autoCropOffset: { x: number; y: number };
    baseImageState: BaseImageState | null;
    hasCropHistory: boolean;
  } {
    return {
      isAutoCropped: this.isAutoCropped,
      autoCropOffset: this.isAutoCropped ? { ...this.autoCropOffset } : this.cropHistoryStack.reduce((offset, item) => ({ x: offset.x + item.offset.x, y: offset.y + item.offset.y }), { x: 0, y: 0 }),
      baseImageState: this.baseImageState
        ? {
            dataUrl: this.baseImageState.dataUrl,
            width: this.baseImageState.width,
            height: this.baseImageState.height,
            uiElements: this.baseImageState.uiElements.map((el) => ({ ...el })),
          }
        : null,
      hasCropHistory: this.cropHistoryStack.length > 0,
    };
  }

  public restoreCropState(
    baseImageState: BaseImageState | null,
    cropInfo: { is_auto_cropped: boolean; offset_x: number; offset_y: number; base_width?: number; base_height?: number } | null
  ): void {
    if (baseImageState) {
      this.baseImageState = {
        dataUrl: baseImageState.dataUrl,
        width: baseImageState.width,
        height: baseImageState.height,
        uiElements: (baseImageState.uiElements ?? []).map((el) => ({ ...el })),
      };
    }
    if (cropInfo) {
      this.isAutoCropped = cropInfo.is_auto_cropped;
      this.autoCropOffset = { x: cropInfo.offset_x, y: cropInfo.offset_y };
      if (baseImageState && !cropInfo.is_auto_cropped) this.cropHistoryStack = [{
        dataUrl: baseImageState.dataUrl, width: baseImageState.width, height: baseImageState.height,
        uiElements: baseImageState.uiElements.map(el => ({ ...el })), offset: { x: cropInfo.offset_x, y: cropInfo.offset_y }, annotations: [],
      }];
      const btnAutoCrop = document.getElementById('btn-autocrop');
      if (btnAutoCrop) {
        btnAutoCrop.classList.toggle('active', this.isAutoCropped);
      }
    }
    this.updateRevertCropButton();
  }

  public getScene(): Scene {
    return JSON.parse(JSON.stringify(this.scene));
  }

  public getCleanScene(): Scene {
    return {
      canvas: { ...this.scene.canvas },
      shadow: this.scene.shadow ?? true,
      annotations: this.scene.annotations.map((ann) => this.cleanAnnotation(ann)),
      hidden_annotations: this.scene.hidden_annotations?.length ? [...this.scene.hidden_annotations] : undefined,
    };
  }

  public hasImage(): boolean {
    return !!this.bgImgEl.src && this.bgImgEl.src.length > 0 && !this.bgImgEl.src.endsWith('/#');
  }

  private cleanAnnotation(anno: any): any {
    const base: any = { type: anno.type };
    if (anno.style) base.style = anno.style;
    if (anno.shadow !== undefined) base.shadow = anno.shadow;

    switch (anno.type) {
      case 'arrow': {
        const res: any = { ...base };
        if (anno.line_style) res.line_style = anno.line_style;
        if (anno.arrowhead) res.arrowhead = anno.arrowhead;
        if (anno.arrow_skin) res.arrow_skin = anno.arrow_skin;
        if (anno.start && anno.end) {
          res.start = [Math.round(anno.start[0]), Math.round(anno.start[1])];
          res.end = [Math.round(anno.end[0]), Math.round(anno.end[1])];
        } else if (anno.target) {
          res.target = anno.target.map((v: number) => Math.round(v));
        }
        if (anno.stroke_width !== undefined && anno.stroke_width !== null && !isNaN(anno.stroke_width)) {
          res.stroke_width = Number(anno.stroke_width);
        }
        if (anno.step !== undefined && anno.step !== '') {
          res.step = parseInt(String(anno.step), 10) || 1;
        }
        const hasText = anno.has_text !== undefined ? Boolean(anno.has_text) : Boolean(anno.text && String(anno.text).trim().length > 0);
        if (hasText && anno.text && String(anno.text).trim() !== '') {
          res.text = String(anno.text);
          const placement = (anno as any).text_placement ?? (anno as any).text_position;
          if (placement) {
            res.text_placement = placement;
          }
        }
        if (anno.position) res.position = anno.position;
        if (anno.box !== undefined) res.box = Boolean(anno.box);
        if (anno.outline !== undefined) res.outline = Boolean(anno.outline);
        return res;
      }
      case 'bezier-arrow': {
        const res: any = { ...base };
        if (anno.line_style) res.line_style = anno.line_style;
        if (anno.arrowhead) res.arrowhead = anno.arrowhead;
        if (anno.arrow_skin) res.arrow_skin = anno.arrow_skin;
        res.start = [Math.round(anno.start[0]), Math.round(anno.start[1])];
        res.control = [Math.round(anno.control[0]), Math.round(anno.control[1])];
        res.end = [Math.round(anno.end[0]), Math.round(anno.end[1])];
        if (anno.stroke_width !== undefined && anno.stroke_width !== null && !isNaN(anno.stroke_width)) {
          res.stroke_width = Number(anno.stroke_width);
        }
        const hasText = anno.has_text !== undefined ? Boolean(anno.has_text) : Boolean(anno.text && String(anno.text).trim().length > 0);
        if (hasText && anno.text && String(anno.text).trim() !== '') {
          res.text = String(anno.text);
          const placement = (anno as any).text_placement ?? (anno as any).text_position;
          if (placement) {
            res.text_placement = placement;
          }
        }
        if (anno.position) res.position = anno.position;
        if (anno.offset !== undefined) res.offset = Number(anno.offset);
        if (anno.t !== undefined) res.t = Number(anno.t);
        if (anno.box !== undefined) res.box = Boolean(anno.box);
        if (anno.outline !== undefined) res.outline = Boolean(anno.outline);
        return res;
      }
      case 'rect':
      case 'rounded-rect':
      case 'circle': {
        const res: any = { ...base };
        res.target = [
          Math.round(anno.target[0]),
          Math.round(anno.target[1]),
          Math.max(1, Math.round(anno.target[2])),
          Math.max(1, Math.round(anno.target[3])),
        ];
        if (anno.stroke_width !== undefined && anno.stroke_width !== null && !isNaN(anno.stroke_width)) {
          res.stroke_width = Number(anno.stroke_width);
        }
        if (anno.type === 'rounded-rect') {
          if (anno.rx !== undefined) res.rx = Number(anno.rx);
          if (anno.ry !== undefined) res.ry = Number(anno.ry);
        }
        return res;
      }
      case 'step-arrow': {
        const res: any = { ...base };
        res.target = [
          Math.round(anno.target[0]),
          Math.round(anno.target[1]),
          Math.max(1, Math.round(anno.target[2])),
          Math.max(1, Math.round(anno.target[3])),
        ];
        if (anno.stroke_width !== undefined && anno.stroke_width !== null && !isNaN(anno.stroke_width)) {
          res.stroke_width = Number(anno.stroke_width);
        }
        if (anno.step !== undefined && anno.step !== '') {
          res.step = parseInt(String(anno.step), 10) || 1;
        }
        if (anno.text) res.text = String(anno.text);
        if (anno.position) res.position = anno.position;
        return res;
      }
      case 'callout':
      case 'label': {
        const res: any = { ...base };
        res.target = [
          Math.round(anno.target[0]),
          Math.round(anno.target[1]),
          Math.max(1, Math.round(anno.target[2])),
          Math.max(1, Math.round(anno.target[3])),
        ];
        res.text = String(anno.text ?? '');
        if (anno.position) res.position = anno.position;
        if (anno.max_width !== undefined) res.max_width = Number(anno.max_width);
        if (anno.outline !== undefined) res.outline = Boolean(anno.outline);
        return res;
      }
      case 'badge': {
        const res: any = { ...base };
        res.target = [
          Math.round(anno.target[0]),
          Math.round(anno.target[1]),
          Math.max(1, Math.round(anno.target[2])),
          Math.max(1, Math.round(anno.target[3])),
        ];
        if (anno.step !== undefined && anno.step !== '') {
          res.step = parseInt(String(anno.step), 10) || 1;
        }
        if (anno.text) res.text = String(anno.text);
        if (anno.position) res.position = anno.position;
        if (anno.arrow !== undefined) res.arrow = Boolean(anno.arrow);
        return res;
      }
      case 'pin': {
        const res: any = { ...base };
        res.target = [
          Math.round(anno.target[0]),
          Math.round(anno.target[1]),
          Math.max(1, Math.round(anno.target[2])),
          Math.max(1, Math.round(anno.target[3])),
        ];
        if (anno.icon) res.icon = String(anno.icon);
        if (anno.text) res.text = String(anno.text);
        if (anno.position) res.position = anno.position;
        if (anno.outline !== undefined) res.outline = Boolean(anno.outline);
        return res;
      }
      case 'spotlight':
      case 'bullseye': {
        const res: any = { ...base };
        res.target = [
          Math.round(anno.target[0]),
          Math.round(anno.target[1]),
          Math.max(1, Math.round(anno.target[2])),
          Math.max(1, Math.round(anno.target[3])),
        ];
        return res;
      }
      case 'divider': {
        const res: any = { ...base };
        res.target = [
          Math.round(anno.target[0]),
          Math.round(anno.target[1]),
          Math.max(1, Math.round(anno.target[2])),
          Math.max(1, Math.round(anno.target[3])),
        ];
        if (anno.position) res.position = anno.position;
        return res;
      }
      default:
        return anno;
    }
  }

  public getSceneJson(): string {
    return JSON.stringify(this.getCleanScene());
  }

  public getBackgroundImageDataUrl(): string {
    return this.bgImgEl.src;
  }

  private pushState() {
    this.undoStack.push(JSON.stringify(this.scene));
    this.redoStack = [];
    this.updateLayerList();
  }

  private removeAnnotation(index: number): void {
    this.scene.annotations.splice(index, 1);
    this.scene.hidden_annotations = (this.scene.hidden_annotations ?? [])
      .filter((hidden) => hidden !== index)
      .map((hidden) => hidden > index ? hidden - 1 : hidden);
    this.selectedIndex = null;
    this.pushState();
    this.render();
    this.updateInspector();
  }

  private toggleLayerVisibility(index: number): void {
    const hidden = new Set(this.scene.hidden_annotations ?? []);
    if (hidden.has(index)) hidden.delete(index);
    else hidden.add(index);
    this.scene.hidden_annotations = [...hidden].sort((a, b) => a - b);
    this.selectMany([...this.selection].filter(i => !hidden.has(i)));
    this.pushState();
    this.render();
    this.updateInspector();
  }

  private moveLayer(index: number, direction: number): void {
    const next = index + direction;
    if (next < 0 || next >= this.scene.annotations.length) return;
    [this.scene.annotations[index], this.scene.annotations[next]] =
      [this.scene.annotations[next], this.scene.annotations[index]];
    this.scene.hidden_annotations = (this.scene.hidden_annotations ?? []).map((hidden) =>
      hidden === index ? next : hidden === next ? index : hidden);
    const selected = [...this.selection].map(i => i === index ? next : i === next ? index : i);
    this.selectedIndex = selected[0] ?? null;
    this.selection = new Set(selected);
    this.pushState();
    this.render();
    this.updateInspector();
  }

  private updateLayerList(): void {
    const annotations = this.scene.annotations;
    this.layerCountEl.textContent = String(annotations.length);
    this.layerListEl.replaceChildren();
    if (annotations.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'layer-empty';
      empty.textContent = 'マークはまだありません';
      this.layerListEl.appendChild(empty);
      return;
    }
    for (let index = annotations.length - 1; index >= 0; index--) {
      const annotation = annotations[index];
      const hidden = this.scene.hidden_annotations?.includes(index) ?? false;
      const row = document.createElement('div');
      row.className = `layer-row${this.selection.has(index) ? ' active' : ''}${hidden ? ' hidden-layer' : ''}`;
      const addButton = (action: string, label: string, title: string): HTMLButtonElement => {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = action === 'select' ? 'layer-select' : 'layer-action';
        button.dataset.layerAction = action;
        button.dataset.layerIndex = String(index);
        button.textContent = label;
        button.title = title;
        row.appendChild(button);
        return button;
      };
      const detail = 'text' in annotation && annotation.text ? `: ${annotation.text}` : '';
      addButton('select', `${index + 1}. ${annotation.type}${detail}`, 'このマークを選択');
      addButton('visibility', hidden ? '◌' : '◉', hidden ? '表示する' : '非表示にする');
      addButton('up', '↑', '前面に移動').disabled = index === annotations.length - 1;
      addButton('down', '↓', '背面に移動').disabled = index === 0;
      this.layerListEl.appendChild(row);
    }
  }

  public undo() {
    if (this.undoStack.length > 1) {
      const current = this.undoStack.pop()!;
      this.redoStack.push(current);
      const prev = this.undoStack[this.undoStack.length - 1];
      this.scene = JSON.parse(prev);
      this.selectedIndex = null;

      if (this.cropHistoryStack.length > 0 &&
          (this.scene.canvas.width !== this.outputWidth || this.scene.canvas.height !== this.outputHeight)) {
        const lastCrop = this.cropHistoryStack.pop()!;
        this.bgImgEl.src = lastCrop.dataUrl;
        this.outputWidth = lastCrop.width;
        this.outputHeight = lastCrop.height;
        this.canvasContainerEl.style.width = `${lastCrop.width}px`;
        this.canvasContainerEl.style.height = `${lastCrop.height}px`;
        this.handlesLayerEl.setAttribute('viewBox', `0 0 ${lastCrop.width} ${lastCrop.height}`);
        if (this.snapGuideLayerEl) {
          this.snapGuideLayerEl.setAttribute('viewBox', `0 0 ${lastCrop.width} ${lastCrop.height}`);
        }
        this.uiElements = lastCrop.uiElements;
        this.updateRevertCropButton();
      }

      this.render();
      this.updateInspector();
    }
  }

  public redo() {
    if (this.redoStack.length > 0) {
      const next = this.redoStack.pop()!;
      this.undoStack.push(next);
      this.scene = JSON.parse(next);
      this.selectedIndex = null;
      this.render();
      this.updateInspector();
    }
  }

  private bindEvents() {
    this.handlesLayerEl.addEventListener('mousedown', (e) => this.handleMouseDown(e));
    this.handlesLayerEl.addEventListener('mouseleave', () => this.clearSnapGuide());
    this.viewportEl.addEventListener('mouseleave', () => this.clearSnapGuide());
    window.addEventListener('mousemove', (e) => this.handleMouseMove(e));
    window.addEventListener('mouseup', () => this.handleMouseUp());
    window.addEventListener('blur', () => this.handleMouseUp());

    // Prevent text / element drag selection artifacts in WebKitGTK
    window.addEventListener('selectstart', (e) => {
      const target = e.target as HTMLElement;
      if (target.tagName !== 'INPUT' && target.tagName !== 'TEXTAREA') {
        e.preventDefault();
      }
    });
    window.addEventListener('dragstart', (e) => {
      const target = e.target as HTMLElement;
      if (target.tagName !== 'A') {
        e.preventDefault();
      }
    });

    // Zoom on Ctrl/Cmd + Mouse Wheel
    this.viewportEl.addEventListener(
      'wheel',
      (e) => {
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          if (e.deltaY < 0) {
            this.zoomIn();
          } else {
            this.zoomOut();
          }
        }
      },
      { passive: false }
    );

    // Crop Overlay events
    if (this.editorCropOverlayEl) {
      this.editorCropOverlayEl.addEventListener('mousedown', (e) => this.handleCropMouseDown(e));
    }
    document.getElementById('btn-apply-crop')?.addEventListener('click', () => this.applyCrop());
    document.getElementById('btn-crop-fit-marks')?.addEventListener('click', () => this.fitCropToAnnotations(32));
    document.getElementById('btn-cancel-crop')?.addEventListener('click', () => this.cancelCropMode());
    document.getElementById('btn-reset-crop')?.addEventListener('click', () => this.resetCrop());
    this.cropPresetSelectEl?.addEventListener('change', () => {
      const val = this.cropPresetSelectEl?.value;
      if (val) this.applyCropPreset(val);
    });

    window.addEventListener('keydown', (e) => {
      if (['INPUT', 'TEXTAREA', 'SELECT'].includes((e.target as HTMLElement).tagName)) {
        return;
      }

      if (this.isCropMode) {
        if (e.key === 'Enter') {
          e.preventDefault();
          this.applyCrop();
        } else if (e.key === 'Escape') {
          e.preventDefault();
          this.cancelCropMode();
        }
        return;
      }

      if (e.key === 'Delete' || e.key === 'Backspace') {
        if (this.selection.size > 0) {
          e.preventDefault();
          const removed = new Set(this.selection);
          const kept = this.scene.annotations.map((_, i) => i).filter(i => !removed.has(i));
          this.scene.annotations = kept.map(i => this.scene.annotations[i]);
          this.scene.hidden_annotations = kept.flatMap((oldIndex, newIndex) =>
            this.scene.hidden_annotations?.includes(oldIndex) ? [newIndex] : []);
          this.selectedIndex = null;
          this.pushState();
          this.render();
          this.updateInspector();
        }
      } else if (e.key === 'Escape') {
        this.marquee = null;
        this.selectMany([]);
      } else if (e.ctrlKey || e.metaKey) {
        if (e.key === 'z') {
          e.preventDefault();
          this.undo();
        } else if (e.key === 'y') {
          e.preventDefault();
          this.redo();
        }
      }
    });
  }

  private getCanvasCoords(e: MouseEvent): { x: number; y: number } {
    const rect = this.handlesLayerEl.getBoundingClientRect();
    const scaleX = this.scene.canvas.width / rect.width;
    const scaleY = this.scene.canvas.height / rect.height;
    return {
      x: (e.clientX - rect.left) * scaleX,
      y: (e.clientY - rect.top) * scaleY,
    };
  }

  private distToSegment(p: { x: number; y: number }, v: { x: number; y: number }, w: { x: number; y: number }): number {
    const l2 = (v.x - w.x) * (v.x - w.x) + (v.y - w.y) * (v.y - w.y);
    if (l2 === 0) return Math.hypot(p.x - v.x, p.y - v.y);
    let t = ((p.x - v.x) * (w.x - v.x) + (p.y - v.y) * (w.y - v.y)) / l2;
    t = Math.max(0, Math.min(1, t));
    return Math.hypot(p.x - (v.x + t * (w.x - v.x)), p.y - (v.y + t * (w.y - v.y)));
  }

  private findSnapPoint(x: number, y: number, threshold: number = 18): {
    x: number;
    y: number;
    snapped: boolean;
    element?: DetectedUiElement;
  } {
    if (!this.isSnapEnabled || this.uiElements.length === 0 || this.isCropMode) {
      return { x, y, snapped: false };
    }

    let bestDist = threshold;
    let bestPt: { x: number; y: number; el: DetectedUiElement } | null = null;

    for (const el of this.uiElements) {
      // Ignore elements whose bounding box is completely away from the cursor
      if (
        x < el.x - threshold ||
        x > el.x + el.width + threshold ||
        y < el.y - threshold ||
        y > el.y + el.height + threshold
      ) {
        continue;
      }

      const pts = [
        // 4 corners
        { x: el.x, y: el.y },
        { x: el.x + el.width, y: el.y },
        { x: el.x, y: el.y + el.height },
        { x: el.x + el.width, y: el.y + el.height },
        // Edges center
        { x: el.x + el.width / 2, y: el.y },
        { x: el.x + el.width / 2, y: el.y + el.height },
        { x: el.x, y: el.y + el.height / 2 },
        { x: el.x + el.width, y: el.y + el.height / 2 },
        // Center
        { x: el.x + el.width / 2, y: el.y + el.height / 2 },
      ];

      for (const pt of pts) {
        const dist = Math.hypot(x - pt.x, y - pt.y);
        if (dist < bestDist) {
          bestDist = dist;
          bestPt = { ...pt, el };
        }
      }
    }

    if (bestPt) {
      return { x: bestPt.x, y: bestPt.y, snapped: true, element: bestPt.el };
    }
    return { x, y, snapped: false };
  }

  /**
   * Find a UI element that matches the dragged rectangle [x1, y1] to [x2, y2].
   * If a UI element has strong overlap or the drag covers it, snaps to that UI element.
   */
  private findSnapElementForRect(
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    threshold: number = 24
  ): {
    snapped: boolean;
    element?: DetectedUiElement;
    rect?: [number, number, number, number];
  } {
    if (!this.isSnapEnabled || this.uiElements.length === 0 || this.isCropMode) {
      return { snapped: false };
    }

    const minX = Math.min(x1, x2);
    const minY = Math.min(y1, y2);
    const maxX = Math.max(x1, x2);
    const maxY = Math.max(y1, y2);
    const w = maxX - minX;
    const h = maxY - minY;

    const cw = this.scene.canvas.width;
    const ch = this.scene.canvas.height;

    let bestElement: DetectedUiElement | null = null;
    let bestScore = Infinity;

    for (const el of this.uiElements) {
      // Exclude giant full-screen / background containers if canvas is reasonably sized
      const isGiant =
        el.width > 800 ||
        el.height > 600 ||
        (cw > 200 && ch > 200 && el.width * el.height > cw * ch * 0.45);
      if (isGiant) continue;

      // 1. Proximity of corners:
      // Does (x1, y1) start near top-left of el AND (x2, y2) near bottom-right?
      const d1 = Math.hypot(minX - el.x, minY - el.y);
      const d2 = Math.hypot(maxX - (el.x + el.width), maxY - (el.y + el.height));
      if (d1 < threshold * 1.5 && d2 < threshold * 1.5) {
        const score = (d1 + d2) / 2;
        if (score < bestScore) {
          bestScore = score;
          bestElement = el;
          continue;
        }
      }

      // 2. Overlap / Bounding box coverage:
      const interLeft = Math.max(minX, el.x);
      const interTop = Math.max(minY, el.y);
      const interRight = Math.min(maxX, el.x + el.width);
      const interBottom = Math.min(maxY, el.y + el.height);

      if (interRight > interLeft && interBottom > interTop) {
        const interArea = (interRight - interLeft) * (interBottom - interTop);
        const elArea = el.width * el.height;
        const dragArea = Math.max(1, w * h);

        const overlapRatio = interArea / elArea;
        const coverageRatio = interArea / dragArea;

        // If drag rectangle mostly covers the element (e.g. dragged over a button)
        if (overlapRatio > 0.45 && coverageRatio > 0.35) {
          const score = (1 - overlapRatio) * 100 + Math.sqrt(elArea) * 0.1;
          if (score < bestScore) {
            bestScore = score;
            bestElement = el;
          }
        }
      }
    }

    if (bestElement) {
      return {
        snapped: true,
        element: bestElement,
        rect: [
          Math.round(bestElement.x),
          Math.round(bestElement.y),
          Math.round(bestElement.width),
          Math.round(bestElement.height),
        ],
      };
    }

    return { snapped: false };
  }

  /**
   * Find a UI element to snap to when moving an existing rectangular annotation.
   */
  private findSnapElementForMove(
    x: number,
    y: number,
    w: number,
    h: number,
    threshold: number = 24
  ): {
    snapped: boolean;
    element?: DetectedUiElement;
    rect?: [number, number, number, number];
  } {
    if (!this.isSnapEnabled || this.uiElements.length === 0 || this.isCropMode) {
      return { snapped: false };
    }

    const centerX = x + w / 2;
    const centerY = y + h / 2;
    const cw = this.scene.canvas.width;
    const ch = this.scene.canvas.height;

    let bestElement: DetectedUiElement | null = null;
    let bestDist = threshold;

    for (const el of this.uiElements) {
      const isGiant =
        el.width > 800 ||
        el.height > 600 ||
        (cw > 200 && ch > 200 && el.width * el.height > cw * ch * 0.45);
      if (isGiant) continue;

      const elCenterX = el.x + el.width / 2;
      const elCenterY = el.y + el.height / 2;

      // Check top-left alignment
      const distTopLeft = Math.hypot(x - el.x, y - el.y);
      // Check center alignment
      const distCenter = Math.hypot(centerX - elCenterX, centerY - elCenterY);

      const minDist = Math.min(distTopLeft, distCenter);
      if (minDist < bestDist) {
        bestDist = minDist;
        bestElement = el;
      }
    }

    if (bestElement) {
      return {
        snapped: true,
        element: bestElement,
        rect: [
          Math.round(bestElement.x),
          Math.round(bestElement.y),
          Math.round(bestElement.width),
          Math.round(bestElement.height),
        ],
      };
    }

    return { snapped: false };
  }

  private findSnapElementAtPoint(x: number, y: number): {
    snapped: boolean; element?: DetectedUiElement; rect?: [number, number, number, number];
  } {
    if (!this.isSnapEnabled || this.isCropMode) return { snapped: false };
    const area = this.scene.canvas.width * this.scene.canvas.height;
    const candidates = this.uiElements.filter(el => el.width > 0 && el.height > 0
      && el.width <= 800 && el.height <= 600 && el.width * el.height < area * 0.45
      && x >= el.x && x <= el.x + el.width && y >= el.y && y <= el.y + el.height);
    candidates.sort((a, b) => a.width * a.height - b.width * b.height);
    const element = candidates[0];
    return element ? { snapped: true, element, rect: [Math.round(element.x), Math.round(element.y), Math.round(element.width), Math.round(element.height)] }
      : this.findSnapElementForMove(x, y, 0, 0);
  }

  public clearSnapGuide(): void {
    this.renderAllSnapTargets();
  }

  /**
   * Keep every snap target visible while snapping is enabled, and highlight the active one.
   */
  private renderAllSnapTargets(snap?: { x: number; y: number; snapped: boolean; element?: DetectedUiElement }) {
    if (!this.snapGuideLayerEl) return;
    if (!this.isSnapEnabled || this.uiElements.length === 0 || this.isCropMode) {
      this.snapGuideLayerEl.innerHTML = '';
      this.activeSnapLayerEl = null;
      return;
    }

    if (!this.activeSnapLayerEl || !this.snapGuideLayerEl.contains(this.activeSnapLayerEl)) {
      const targets = this.uiElements.map((el) => `
        <rect class="snap-target-rect" x="${el.x}" y="${el.y}"
          width="${el.width}" height="${el.height}" rx="3" />
      `).join('');
      this.snapGuideLayerEl.innerHTML = `<g class="snap-targets">${targets}</g><g class="active-snap-layer"></g>`;
      this.activeSnapLayerEl = this.snapGuideLayerEl.querySelector('.active-snap-layer');
    }

    if (!snap?.snapped || !snap.element || !this.activeSnapLayerEl) {
      if (this.activeSnapLayerEl) this.activeSnapLayerEl.innerHTML = '';
      return;
    }

    const el = snap.element;
    const badgeText = el.name ? `[${el.role}] ${el.name}` : `[${el.role}]`;
    const badgeW = Math.max(50, Math.min(200, badgeText.length * 7 + 10));
    const badgeY = Math.max(0, el.y - 18);

    this.activeSnapLayerEl.innerHTML = `
      <rect
        class="snap-target-rect active"
        x="${el.x}"
        y="${el.y}"
        width="${el.width}"
        height="${el.height}"
        pointer-events="none"
      />
      <g class="active-snap-indicator">
        <circle cx="${snap.x}" cy="${snap.y}" r="6" fill="#38bdf8" stroke="#ffffff" stroke-width="2" />
        <circle cx="${snap.x}" cy="${snap.y}" r="2" fill="#ffffff" />
        <rect x="${el.x}" y="${badgeY}" width="${badgeW}" height="16" fill="#0284c7" rx="3" opacity="0.9" />
        <text x="${el.x + 5}" y="${badgeY + 12}" fill="#ffffff" font-size="10" font-weight="600" font-family="sans-serif">${escapeXml(badgeText)}</text>
      </g>
    `;
  }

  private findAnnotationAt(x: number, y: number): number | null {
    for (let i = this.scene.annotations.length - 1; i >= 0; i--) {
      if (this.scene.hidden_annotations?.includes(i)) continue;
      const anno = this.scene.annotations[i];
      if (anno.type === 'arrow') {
        const arrow = anno as ArrowAnnotation;
        const start = arrow.start ?? [100, 100];
        const end = arrow.end ?? [200, 200];
        if (this.distToSegment({ x, y }, { x: start[0], y: start[1] }, { x: end[0], y: end[1] }) <= 16) {
          return i;
        }
      } else if (anno.type === 'bezier-arrow') {
        const b = anno as BezierArrowAnnotation;
        const p0 = { x: b.start[0], y: b.start[1] };
        const p1 = { x: b.control[0], y: b.control[1] };
        const p2 = { x: b.end[0], y: b.end[1] };
        for (let step = 0; step <= 10; step++) {
          const t = step / 10;
          const cx = (1 - t) * (1 - t) * p0.x + 2 * (1 - t) * t * p1.x + t * t * p2.x;
          const cy = (1 - t) * (1 - t) * p0.y + 2 * (1 - t) * t * p1.y + t * t * p2.y;
          if (Math.hypot(x - cx, y - cy) <= 18) {
            return i;
          }
        }
      } else if (anno.type === 'pin' && Array.isArray((anno as any).target)) {
        const [tx, ty, tw, th] = (anno as any).target;
        // Check target bounds
        const pad = 8;
        if (x >= tx - pad && x <= tx + tw + pad && y >= ty - pad && y <= ty + th + pad) {
          return i;
        }
        // Also check pin head circle based on position
        const pos = (anno as any).position || 'top';
        let hx = tx + tw / 2;
        let hy = ty - 24;
        if (pos === 'bottom') hy = ty + th + 24;
        else if (pos === 'left') { hx = tx - 24; hy = ty + th / 2; }
        else if (pos === 'right') { hx = tx + tw + 24; hy = ty + th / 2; }
        if (Math.hypot(x - hx, y - hy) <= 24) {
          return i;
        }
      } else if ('target' in anno && Array.isArray((anno as any).target)) {
        const [tx, ty, tw, th] = (anno as any).target;
        const pad = 8;
        if (x >= tx - pad && x <= tx + tw + pad && y >= ty - pad && y <= ty + th + pad) {
          return i;
        }
      }
    }
    return null;
  }

  private handleMouseDown(e: MouseEvent) {
    if (e.button !== 0 || this.isCropMode) return;
    const target = e.target as SVGElement;
    const coords = this.getCanvasCoords(e);

    // 1. Check if clicking on an interactive handle or selection proxy
    if (target.dataset.handleType && target.dataset.annotationIndex !== undefined) {
      const idx = parseInt(target.dataset.annotationIndex, 10);
      if (target.dataset.handleType === 'select-anno') {
        if (e.shiftKey) { this.toggleSelection(idx); return; }
        this.selectedIndex = idx;
        this.isDraggingAnnotation = true;
        this.dragStartX = coords.x;
        this.dragStartY = coords.y;
        this.initialAnnotationState = JSON.parse(JSON.stringify(this.scene.annotations[idx]));
        this.updateHandles();
        this.updateInspector();
        e.stopPropagation();
        return;
      }

      this.isDraggingHandle = true;
      this.activeHandle = {
        type: target.dataset.handleType,
        index: idx,
      };
      this.dragStartX = coords.x;
      this.dragStartY = coords.y;
      this.initialAnnotationState = JSON.parse(JSON.stringify(this.scene.annotations[idx]));
      e.stopPropagation();
      return;
    }

    // 2. Creating a new mark
    if (this.activeTool !== 'select') {
      const snap = e.altKey ? { x: coords.x, y: coords.y, snapped: false } : this.findSnapPoint(coords.x, coords.y);
      this.isCreating = true;
      this.createStartX = snap.x;
      this.createStartY = snap.y;

      const newAnno = this.createDefaultAnnotation(this.activeTool, snap.x, snap.y);
      if (snap.snapped && snap.element && 'target' in newAnno) {
        const cw = this.scene.canvas.width;
        const ch = this.scene.canvas.height;
        const el = snap.element;
        const isNotHuge =
          el.width <= 650 &&
          el.height <= 450 &&
          (cw < 200 || ch < 200 || (
            el.width < cw * 0.65 &&
            el.height < ch * 0.65 &&
            el.width * el.height < cw * ch * 0.40
          ));
        if (isNotHuge) {
          (newAnno as any).target = [
            Math.round(el.x),
            Math.round(el.y),
            Math.round(el.width),
            Math.round(el.height),
          ];
        }
      }
      this.clearSnapGuide();
      if (this.activeTool === 'arrow') {
        (newAnno as ArrowAnnotation).start = [Math.round(snap.x), Math.round(snap.y)];
        (newAnno as ArrowAnnotation).end = [Math.round(snap.x + 1), Math.round(snap.y + 1)];
      }
      this.scene.annotations.push(newAnno);
      this.selectedIndex = this.scene.annotations.length - 1;
      this.initialAnnotationState = JSON.parse(JSON.stringify(newAnno));
      this.render();
      return;
    }

    // 3. Selection mode: hit-test existing annotations fallback
    const hitIdx = this.findAnnotationAt(coords.x, coords.y);
    if (hitIdx !== null) {
      if (e.shiftKey) { this.toggleSelection(hitIdx); return; }
      this.selectedIndex = hitIdx;
      this.isDraggingAnnotation = true;
      this.dragStartX = coords.x;
      this.dragStartY = coords.y;
      this.initialAnnotationState = JSON.parse(JSON.stringify(this.scene.annotations[hitIdx]));
      this.updateHandles();
      this.updateInspector();
      return;
    }

    this.marquee = {x:coords.x,y:coords.y,endX:coords.x,endY:coords.y,initial:e.shiftKey ? [...this.selection] : []};
    this.selectedIndex = null;
    this.updateHandles();
    this.updateInspector();
  }

  private handleMouseMove(e: MouseEvent) {
    if (this.isCropMode) {
      if (this.isDraggingCrop) {
        this.handleCropMouseMove(e);
      }
      return;
    }
    const coords = this.getCanvasCoords(e);

    if (this.marquee) { this.marquee.endX = coords.x; this.marquee.endY = coords.y; this.updateHandles(); return; }
    // Dragging an interactive handle
    if (this.isDraggingHandle && this.activeHandle && this.initialAnnotationState) {
      const snap = e.altKey ? { x: coords.x, y: coords.y, snapped: false } : this.findSnapPoint(coords.x, coords.y);
      this.renderAllSnapTargets(snap);

      const idx = this.activeHandle.index;
      const anno = this.scene.annotations[idx];
      const initial = this.initialAnnotationState;
      const curX = snap.x;
      const curY = snap.y;
      const dx = curX - this.dragStartX;
      const dy = curY - this.dragStartY;

      if (anno.type === 'arrow') {
        const arrow = anno as ArrowAnnotation;
        if (this.activeHandle.type === 'start') {
          arrow.start = [Math.round(snap.snapped ? curX : (initial.start?.[0] ?? 0) + dx), Math.round(snap.snapped ? curY : (initial.start?.[1] ?? 0) + dy)];
        } else if (this.activeHandle.type === 'end') {
          arrow.end = [Math.round(snap.snapped ? curX : (initial.end?.[0] ?? 100) + dx), Math.round(snap.snapped ? curY : (initial.end?.[1] ?? 100) + dy)];
        }
      } else if (anno.type === 'bezier-arrow') {
        const b = anno as BezierArrowAnnotation;
        if (this.activeHandle.type === 'start') {
          b.start = [Math.round(snap.snapped ? curX : initial.start[0] + dx), Math.round(snap.snapped ? curY : initial.start[1] + dy)];
        } else if (this.activeHandle.type === 'end') {
          b.end = [Math.round(snap.snapped ? curX : initial.end[0] + dx), Math.round(snap.snapped ? curY : initial.end[1] + dy)];
        } else if (this.activeHandle.type === 'control') {
          b.control = [Math.round(initial.control[0] + dx), Math.round(initial.control[1] + dy)];
        }
      } else if ('target' in anno) {
        const target = (anno as any).target;
        const initTarget = initial.target;
        if (this.activeHandle.type === 'move') {
          const rawX = Math.max(0, Math.round(initTarget[0] + coords.x - this.dragStartX));
          const rawY = Math.max(0, Math.round(initTarget[1] + coords.y - this.dragStartY));
          const moved = !e.altKey ? this.findSnapElementForMove(rawX, rawY, initTarget[2], initTarget[3]) : { snapped: false };
          target[0] = moved.rect?.[0] ?? rawX;
          target[1] = moved.rect?.[1] ?? rawY;
          if (moved.rect && ['badge', 'step-arrow', 'label', 'rect', 'rounded-rect', 'spotlight'].includes(anno.type)) {
            target[2] = moved.rect[2]; target[3] = moved.rect[3];
          }
          this.renderAllSnapTargets(moved.rect ? { x: target[0], y: target[1], snapped: true, element: moved.element } : undefined);
        } else if (this.activeHandle.type === 'se') {
          let rectSnapped = false;
          if (!e.altKey && this.isSnapEnabled) {
            const rectSnap = this.findSnapElementForRect(initTarget[0], initTarget[1], coords.x, coords.y);
            if (rectSnap.snapped && rectSnap.element && rectSnap.rect) {
              target[2] = Math.max(20, rectSnap.rect[0] + rectSnap.rect[2] - initTarget[0]);
              target[3] = Math.max(20, rectSnap.rect[1] + rectSnap.rect[3] - initTarget[1]);
              this.renderAllSnapTargets({ x: rectSnap.rect[0], y: rectSnap.rect[1], snapped: true, element: rectSnap.element });
              rectSnapped = true;
            }
          }
          if (!rectSnapped) {
            const newW = snap.snapped ? Math.max(20, curX - initTarget[0]) : Math.max(20, Math.round(initTarget[2] + dx));
            const newH = snap.snapped ? Math.max(20, curY - initTarget[1]) : Math.max(20, Math.round(initTarget[3] + dy));
            target[2] = Math.round(newW);
            target[3] = Math.round(newH);
          }
        }
      }

      this.scheduleRender();
      return;
    }

    // Dragging to move the entire annotation
    if (this.isDraggingAnnotation && this.selectedIndex !== null && this.initialAnnotationState) {
      const anno = this.scene.annotations[this.selectedIndex];
      const initial = this.initialAnnotationState;
      const dx = coords.x - this.dragStartX;
      const dy = coords.y - this.dragStartY;

      if (anno.type === 'arrow') {
        this.renderAllSnapTargets();
        const arrow = anno as ArrowAnnotation;
        if (initial.start && initial.end) {
          arrow.start = [Math.round(initial.start[0] + dx), Math.round(initial.start[1] + dy)];
          arrow.end = [Math.round(initial.end[0] + dx), Math.round(initial.end[1] + dy)];
        }
      } else if (anno.type === 'bezier-arrow') {
        this.renderAllSnapTargets();
        const b = anno as BezierArrowAnnotation;
        b.start = [Math.round(initial.start[0] + dx), Math.round(initial.start[1] + dy)];
        b.control = [Math.round(initial.control[0] + dx), Math.round(initial.control[1] + dy)];
        b.end = [Math.round(initial.end[0] + dx), Math.round(initial.end[1] + dy)];
      } else if ('target' in anno && Array.isArray((anno as any).target)) {
        const target = (anno as any).target;
        const initTarget = initial.target;
        const rawX = Math.max(0, Math.round(initTarget[0] + dx));
        const rawY = Math.max(0, Math.round(initTarget[1] + dy));

        if (!e.altKey && this.isSnapEnabled) {
          const isBadge = anno.type === 'badge' || anno.type === 'step-arrow' || anno.type === 'label';
          const snap = isBadge
            ? this.findSnapElementAtPoint(coords.x, coords.y)
            : this.findSnapElementForMove(rawX, rawY, initTarget[2], initTarget[3]);
          if (snap.snapped && snap.element && snap.rect) {
            target[0] = snap.rect[0];
            target[1] = snap.rect[1];
            if (isBadge || anno.type === 'rect' || anno.type === 'rounded-rect' || anno.type === 'spotlight') {
              target[2] = snap.rect[2];
              target[3] = snap.rect[3];
            }
            this.renderAllSnapTargets({ x: snap.rect[0], y: snap.rect[1], snapped: true, element: snap.element });
          } else {
            target[0] = rawX;
            target[1] = rawY;
            this.renderAllSnapTargets();
          }
        } else {
          target[0] = rawX;
          target[1] = rawY;
          this.renderAllSnapTargets();
        }
      }

      this.scheduleRender();
      return;
    }

    // Dragging to create a new mark
    if (this.isCreating && this.selectedIndex !== null) {
      const snap = e.altKey ? { x: coords.x, y: coords.y, snapped: false } : this.findSnapPoint(coords.x, coords.y);

      const endX = snap.x;
      const endY = snap.y;
      const anno = this.scene.annotations[this.selectedIndex];
      const width = endX - this.createStartX;
      const height = endY - this.createStartY;

      if (anno.type === 'arrow') {
        this.renderAllSnapTargets(snap);
        const arrow = anno as ArrowAnnotation;
        arrow.start = [Math.round(this.createStartX), Math.round(this.createStartY)];
        arrow.end = [Math.round(endX), Math.round(endY)];
      } else if (anno.type === 'bezier-arrow') {
        this.renderAllSnapTargets(snap);
        const b = anno as BezierArrowAnnotation;
        b.start = [Math.round(this.createStartX), Math.round(this.createStartY)];
        b.end = [Math.round(endX), Math.round(endY)];
        b.control = [
          Math.round((this.createStartX + endX) / 2),
          Math.round((this.createStartY + endY) / 2 - 50),
        ];
      } else if ('target' in anno) {
        let rectSnapped = false;
        if (!e.altKey && this.isSnapEnabled) {
          const rectSnap = this.findSnapElementForRect(this.createStartX, this.createStartY, coords.x, coords.y);
          if (rectSnap.snapped && rectSnap.element && rectSnap.rect) {
            (anno as any).target = [rectSnap.rect[0], rectSnap.rect[1], rectSnap.rect[2], rectSnap.rect[3]];
            this.renderAllSnapTargets({ x: rectSnap.rect[0], y: rectSnap.rect[1], snapped: true, element: rectSnap.element });
            rectSnapped = true;
          }
        }

        if (!rectSnapped) {
          this.renderAllSnapTargets(snap);
          const x = Math.min(this.createStartX, endX);
          const y = Math.min(this.createStartY, endY);
          const w = Math.max(20, Math.abs(width));
          const h = Math.max(20, Math.abs(height));
          (anno as any).target = [Math.round(x), Math.round(y), Math.round(w), Math.round(h)];
        }
      }

      this.scheduleRender();
      return;
    }

    // Hovering mode (not dragging)
    const cw = this.scene.canvas.width;
    const ch = this.scene.canvas.height;
    const isInsideCanvas = coords.x >= 0 && coords.x <= cw && coords.y >= 0 && coords.y <= ch;

    if (this.activeTool !== 'select' && this.isSnapEnabled && isInsideCanvas) {
      const snap = e.altKey ? { x: coords.x, y: coords.y, snapped: false } : this.findSnapPoint(coords.x, coords.y);
      this.renderAllSnapTargets(snap);
    } else {
      this.clearSnapGuide();
    }
  }

  private handleMouseUp() {
    if (this.marquee) {
      const m = this.marquee; this.marquee = null;
      const indices = new Set(m.initial);
      if (Math.hypot(m.endX-m.x,m.endY-m.y)>3) this.scene.annotations.forEach((_,i)=>{
        if (this.scene.hidden_annotations?.includes(i)) return;
        const b = this.bounds(i);
        if (b.x<=Math.max(m.x,m.endX) && b.x+b.width>=Math.min(m.x,m.endX) && b.y<=Math.max(m.y,m.endY) && b.y+b.height>=Math.min(m.y,m.endY)) indices.add(i);
      });
      this.selectMany([...indices]); return;
    }
    if (this.isCropMode) {
      if (this.isDraggingCrop) {
        this.handleCropMouseUp();
      }
      return;
    }
    try {
      this.clearSnapGuide();

      if (this.isCreating && this.selectedIndex !== null) {
        const anno = this.scene.annotations[this.selectedIndex];
        if (anno.type === 'arrow') {
          const arrow = anno as ArrowAnnotation;
          const dist = Math.hypot(
            (arrow.end?.[0] ?? 0) - (arrow.start?.[0] ?? 0),
            (arrow.end?.[1] ?? 0) - (arrow.start?.[1] ?? 0)
          );
          if (dist < 10) {
            const cw = this.scene.canvas.width;
            const ch = this.scene.canvas.height;
            let sx = this.createStartX - 80;
            let sy = this.createStartY - 60;
            if (sx < 10) sx = Math.min(cw - 10, this.createStartX + 80);
            if (sy < 10) sy = Math.min(ch - 10, this.createStartY + 60);
            arrow.start = [Math.round(sx), Math.round(sy)];
            arrow.end = [Math.round(this.createStartX), Math.round(this.createStartY)];
            this.render();
          }
        }
      }

      if (this.isDraggingHandle || this.isDraggingAnnotation || this.isCreating) {
        const wasCreating = this.isCreating;
        this.isDraggingHandle = false;
        this.isDraggingAnnotation = false;
        this.isCreating = false;
        this.activeHandle = null;
        this.initialAnnotationState = null;
        this.clearSnapGuide();
        if (wasCreating) {
          // Finalize annotation: completely clear selection so no blue lines, handles, or boxes remain!
          this.selectedIndex = null;
        }
        this.setTool('select');
        this.pushState();
        this.updateInspector();
        this.updateHandles();
      }
    } finally {
      this.clearSnapGuide();
      this.isDraggingHandle = false;
      this.isDraggingAnnotation = false;
      this.isCreating = false;
      this.activeHandle = null;
      this.initialAnnotationState = null;
    }
  }

  private createDefaultAnnotation(type: string, x: number, y: number): Annotation {
    const defaultTarget: [number, number, number, number] = [Math.round(x - 50), Math.round(y - 25), 100, 50];
    const nextStep = () => this.scene.annotations.reduce((maximum, annotation) => {
      if (annotation.type !== 'badge' && annotation.type !== 'step-arrow') return maximum;
      const step = Number(annotation.step ?? 1);
      return Number.isSafeInteger(step) && step > 0 ? Math.max(maximum, step) : maximum;
    }, 0) + 1;

    switch (type) {
      case 'arrow':
        return {
          type: 'arrow',
          start: [Math.round(x - 80), Math.round(y - 60)],
          end: [Math.round(x), Math.round(y)],
          style: 'primary',
          stroke_width: 4,
          text: '',
          has_text: false,
          box: true,
          position: 'auto',
        };
      case 'bezier-arrow':
        return {
          type: 'bezier-arrow',
          start: [Math.round(x - 100), Math.round(y)],
          control: [Math.round(x - 50), Math.round(y - 60)],
          end: [Math.round(x), Math.round(y)],
          text: '',
          has_text: false,
          style: 'primary',
          stroke_width: 4,
          box: true,
          position: 'auto',
        };
      case 'callout':
        return {
          type: 'callout',
          target: defaultTarget,
          text: '説明テキスト',
          position: 'auto',
          style: 'primary',
        };
      case 'pin':
        return {
          type: 'pin',
          target: defaultTarget,
          text: '注目ポイント',
          position: 'top',
          style: 'primary',
        };
      case 'label':
        return {
          type: 'label',
          target: defaultTarget,
          text: 'ラベル',
          position: 'top',
          style: 'primary',
        };
      case 'badge':
        return {
          type: 'badge',
          target: defaultTarget,
          step: nextStep(),
          style: 'step',
          position: 'top',
        };
      case 'step-arrow':
        return {
          type: 'step-arrow',
          target: defaultTarget,
          step: nextStep(),
          style: 'step',
          position: 'right',
        };
      case 'rect':
        return {
          type: 'rect',
          target: defaultTarget,
          style: 'primary',
        };
      case 'rounded-rect':
        return {
          type: 'rounded-rect',
          target: defaultTarget,
          rx: 12,
          ry: 12,
          style: 'primary',
        };
      case 'circle':
        return {
          type: 'circle',
          target: defaultTarget,
          style: 'primary',
        };
      case 'spotlight':
        return {
          type: 'spotlight',
          target: defaultTarget,
          style: 'primary',
        };
      case 'bullseye':
        return {
          type: 'bullseye',
          target: [Math.round(x - 20), Math.round(y - 20), 40, 40],
          style: 'danger',
        };
      case 'divider':
        return {
          type: 'divider',
          target: defaultTarget,
          position: 'bottom',
          style: 'secondary',
        };
      default:
        return {
          type: 'rect',
          target: defaultTarget,
          style: 'primary',
        };
    }
  }

  public scheduleRender() {
    if (this.rafId !== null) return;
    this.rafId = requestAnimationFrame(() => {
      this.rafId = null;
      if (this.isRendering) {
        this.renderQueued = true;
      } else {
        this.executeRender();
      }
    });
  }

  public async render() {
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
    await this.executeRender();
  }

  private async executeRender() {
    if (this.isRendering) {
      this.renderQueued = true;
      return;
    }
    this.isRendering = true;
    try {
      const cleanScene = this.getCleanScene();
      const svg = await this.renderCallback(JSON.stringify(cleanScene));
      this.svgLayerEl.innerHTML = svg;
    } catch (e) {
      console.warn('SVG render warning:', e);
    } finally {
      this.isRendering = false;
      if (this.renderQueued) {
        this.renderQueued = false;
        this.scheduleRender();
      }
    }

    // Never recreate handles while dragging! Recreating SVG DOM during active drag
    // destroys the event target, breaking mouse tracking/capture and causing an apparent UI hang.
    if (!this.isDraggingHandle && !this.isDraggingAnnotation && !this.isCreating && !this.isCropMode) {
      this.updateHandles();
    }
  }

  private updateHandles() {
    this.handlesLayerEl.innerHTML = '';
    if (this.isCropMode) {
      return;
    }

    // 1. Full-canvas transparent background rect to reliably catch clicks/deselects
    const bgRect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    bgRect.setAttribute('x', '0');
    bgRect.setAttribute('y', '0');
    bgRect.setAttribute('width', this.scene.canvas.width.toString());
    bgRect.setAttribute('height', this.scene.canvas.height.toString());
    bgRect.setAttribute('fill', 'transparent');
    bgRect.dataset.handleType = 'background';
    bgRect.style.cursor = this.activeTool === 'select' ? 'default' : 'crosshair';
    this.handlesLayerEl.appendChild(bgRect);

    // 2. Generous hit-test proxies for all annotations so clicking on any arrow or shape re-selects it!
    for (let i = 0; i < this.scene.annotations.length; i++) {
      if (this.scene.hidden_annotations?.includes(i)) continue;
      const anno = this.scene.annotations[i];
      if (anno.type === 'arrow') {
        const arrow = anno as ArrowAnnotation;
        const start = arrow.start ?? [100, 100];
        const end = arrow.end ?? [200, 200];
        const hitLine = document.createElementNS('http://www.w3.org/2000/svg', 'line');
        hitLine.setAttribute('x1', start[0].toString());
        hitLine.setAttribute('y1', start[1].toString());
        hitLine.setAttribute('x2', end[0].toString());
        hitLine.setAttribute('y2', end[1].toString());
        hitLine.setAttribute('stroke', 'transparent');
        hitLine.setAttribute('stroke-width', '24');
        hitLine.setAttribute('stroke-linecap', 'round');
        hitLine.style.cursor = 'pointer';
        hitLine.dataset.handleType = 'select-anno';
        hitLine.dataset.annotationIndex = i.toString();
        this.handlesLayerEl.appendChild(hitLine);
        {
          const length = Math.hypot(end[0] - start[0], end[1] - start[1]);
          const skin = arrow.arrow_skin ?? 'classic';
          const strokeWidth = arrow.stroke_width ?? 4;
          const sketchOutline = Math.min(Math.max(strokeWidth * 0.2, 1), 2);
          const headLength = skin === 'classic' || skin === 'sketch' ? Math.min(Math.max(strokeWidth * 3.5, 18), 30, length * 0.45) : Math.min(Math.max(length * 0.26, 18), 90, length * 0.45);
          const headHalf = skin === 'classic' ? Math.min(Math.max(strokeWidth * 2.3, 10), 22) : skin === 'sketch' ? Math.min(Math.min(Math.max(strokeWidth * 2.3, 10), 22) - sketchOutline * 0.5, length * 0.26) : Math.min(Math.max(length * 0.12, 10), 65, length * 0.25);
          const radius = headHalf + 8;
          const neckFraction = 1 - headLength / Math.max(length, 1);
          const headHit = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
          headHit.setAttribute('cx', String(start[0] + (end[0] - start[0]) * neckFraction));
          headHit.setAttribute('cy', String(start[1] + (end[1] - start[1]) * neckFraction));
          headHit.setAttribute('r', String(radius));
          headHit.setAttribute('fill', 'transparent');
          headHit.style.cursor = 'pointer';
          headHit.dataset.handleType = 'select-anno';
          headHit.dataset.annotationIndex = i.toString();
          this.handlesLayerEl.appendChild(headHit);
        }
      } else if (anno.type === 'bezier-arrow') {
        const b = anno as BezierArrowAnnotation;
        const hitPath = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        hitPath.setAttribute('d', `M ${b.start[0]} ${b.start[1]} Q ${b.control[0]} ${b.control[1]} ${b.end[0]} ${b.end[1]}`);
        hitPath.setAttribute('fill', 'none');
        hitPath.setAttribute('stroke', 'transparent');
        hitPath.setAttribute('stroke-width', '24');
        hitPath.setAttribute('stroke-linecap', 'round');
        hitPath.style.cursor = 'pointer';
        hitPath.dataset.handleType = 'select-anno';
        hitPath.dataset.annotationIndex = i.toString();
        this.handlesLayerEl.appendChild(hitPath);
        {
          const length = Math.hypot(b.control[0] - b.start[0], b.control[1] - b.start[1]) + Math.hypot(b.end[0] - b.control[0], b.end[1] - b.control[1]);
          const skin = b.arrow_skin ?? 'classic';
          const strokeWidth = b.stroke_width ?? 4;
          const sketchOutline = Math.min(Math.max(strokeWidth * 0.2, 1), 2);
          const headLength = skin === 'classic' || skin === 'sketch' ? Math.min(Math.max(strokeWidth * 3.5, 18), 30, length * 0.45) : Math.min(Math.max(length * 0.26, 18), 90, length * 0.45);
          const headHalf = skin === 'classic' ? Math.min(Math.max(strokeWidth * 2.3, 10), 22) : skin === 'sketch' ? Math.min(Math.min(Math.max(strokeWidth * 2.3, 10), 22) - sketchOutline * 0.5, length * 0.26) : Math.min(Math.max(length * 0.12, 10), 65, length * 0.25);
          const radius = headHalf + 8;
          const t = 1 - headLength / Math.max(length, 1);
          const inv = 1 - t;
          const headHit = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
          headHit.setAttribute('cx', String(inv * inv * b.start[0] + 2 * inv * t * b.control[0] + t * t * b.end[0]));
          headHit.setAttribute('cy', String(inv * inv * b.start[1] + 2 * inv * t * b.control[1] + t * t * b.end[1]));
          headHit.setAttribute('r', String(radius));
          headHit.setAttribute('fill', 'transparent');
          headHit.style.cursor = 'pointer';
          headHit.dataset.handleType = 'select-anno';
          headHit.dataset.annotationIndex = i.toString();
          this.handlesLayerEl.appendChild(headHit);
        }
      } else if ('target' in anno && Array.isArray((anno as any).target)) {
        const [x, y, w, h] = (anno as any).target;
        const hitRect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
        hitRect.setAttribute('x', (x - 4).toString());
        hitRect.setAttribute('y', (y - 4).toString());
        hitRect.setAttribute('width', (w + 8).toString());
        hitRect.setAttribute('height', (h + 8).toString());
        hitRect.setAttribute('fill', 'transparent');
        hitRect.setAttribute('stroke', 'transparent');
        hitRect.setAttribute('stroke-width', '8');
        hitRect.style.cursor = 'pointer';
        hitRect.dataset.handleType = 'select-anno';
        hitRect.dataset.annotationIndex = i.toString();
        this.handlesLayerEl.appendChild(hitRect);

        if (anno.type === 'pin') {
          const pos = (anno as any).position || 'top';
          let hx = x + w / 2;
          let hy = y - 24;
          if (pos === 'bottom') hy = y + h + 24;
          else if (pos === 'left') { hx = x - 24; hy = y + h / 2; }
          else if (pos === 'right') { hx = x + w + 24; hy = y + h / 2; }
          const hitHead = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
          hitHead.setAttribute('cx', hx.toString());
          hitHead.setAttribute('cy', hy.toString());
          hitHead.setAttribute('r', '24');
          hitHead.setAttribute('fill', 'transparent');
          hitHead.style.cursor = 'pointer';
          hitHead.dataset.handleType = 'select-anno';
          hitHead.dataset.annotationIndex = i.toString();
          this.handlesLayerEl.appendChild(hitHead);
        }
      }
    }

    // Use the renderer's final badge circles: automatic placement can move a
    // badge away from its target rectangle to avoid other annotations.
    const visibleIndices = this.scene.annotations.map((_, index) => index)
      .filter(index => !this.scene.hidden_annotations?.includes(index));
    for (const badge of this.svgLayerEl.querySelectorAll<SVGElement>('circle[data-badge-index], rect[data-label-index]')) {
      const isLabel = badge.dataset.labelIndex !== undefined;
      const index = visibleIndices[Number(isLabel ? badge.dataset.labelIndex : badge.dataset.badgeIndex)];
      if (index === undefined) continue;
      const annotation = this.scene.annotations[index];
      if (annotation.type !== 'badge' && annotation.type !== 'step-arrow' && annotation.type !== 'label') continue;
      const hit = document.createElementNS('http://www.w3.org/2000/svg', isLabel ? 'rect' : 'circle');
      if (isLabel) {
        for (const attribute of ['x', 'y', 'width', 'height', 'rx', 'ry']) hit.setAttribute(attribute, badge.getAttribute(attribute)!);
      } else {
        hit.setAttribute('cx', badge.getAttribute('cx')!);
        hit.setAttribute('cy', badge.getAttribute('cy')!);
        hit.setAttribute('r', String(Number(badge.getAttribute('r')) + 3));
      }
      hit.setAttribute('fill', 'transparent');
      hit.style.cursor = 'pointer';
      hit.dataset.handleType = 'select-anno';
      hit.dataset.annotationIndex = String(index);
      this.handlesLayerEl.appendChild(hit);
    }

    if (this.marquee) {
      const m=this.marquee; this.selectionBox(Math.min(m.x,m.endX),Math.min(m.y,m.endY),Math.abs(m.endX-m.x),Math.abs(m.endY-m.y));
    }
    if (this.selection.size>1) {
      for(const i of this.selection) { const b=this.bounds(i); this.selectionBox(b.x,b.y,b.width,b.height); }
      return;
    }
    if (this.selectedIndex === null || this.selectedIndex >= this.scene.annotations.length
        || this.scene.hidden_annotations?.includes(this.selectedIndex)) {
      return;
    }

    const idx = this.selectedIndex;
    const anno = this.scene.annotations[idx];

    if (anno.type === 'arrow') {
      const arrow = anno as ArrowAnnotation;
      const start = arrow.start ?? [100, 100];
      const end = arrow.end ?? [200, 200];

      // Dashed line selection indicator
      const selLine = document.createElementNS('http://www.w3.org/2000/svg', 'line');
      selLine.setAttribute('x1', start[0].toString());
      selLine.setAttribute('y1', start[1].toString());
      selLine.setAttribute('x2', end[0].toString());
      selLine.setAttribute('y2', end[1].toString());
      selLine.setAttribute('stroke', '#3b82f6');
      selLine.setAttribute('stroke-width', '1.5');
      selLine.setAttribute('stroke-dasharray', '4 4');
      selLine.setAttribute('pointer-events', 'none');
      this.handlesLayerEl.appendChild(selLine);

      // Show draggable start (green) and end (red) handle circles on the arrow
      this.addSvgHandle(start[0], start[1], 'start', idx, 'handle-point start');
      this.addSvgHandle(end[0], end[1], 'end', idx, 'handle-point end');
    } else if (anno.type === 'bezier-arrow') {
      const b = anno as BezierArrowAnnotation;
      this.addSvgLine(b.start[0], b.start[1], b.control[0], b.control[1], 'handle-line');
      this.addSvgLine(b.control[0], b.control[1], b.end[0], b.end[1], 'handle-line');

      this.addSvgHandle(b.start[0], b.start[1], 'start', idx, 'handle-point start');
      this.addSvgHandle(b.control[0], b.control[1], 'control', idx, 'handle-point control');
      this.addSvgHandle(b.end[0], b.end[1], 'end', idx, 'handle-point end');
    } else if ('target' in anno) {
      const [x, y, w, h] = (anno as any).target;

      // Draw bounding box
      const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
      rect.setAttribute('x', x.toString());
      rect.setAttribute('y', y.toString());
      rect.setAttribute('width', w.toString());
      rect.setAttribute('height', h.toString());
      rect.setAttribute('class', 'handle-bbox');
      rect.dataset.handleType = 'move';
      rect.dataset.annotationIndex = idx.toString();
      this.handlesLayerEl.appendChild(rect);

      if (anno.type !== 'badge' && anno.type !== 'step-arrow' && anno.type !== 'label') {
        // Resize and move handles are unnecessary for numbered badges.
        this.addSvgHandle(x + w, y + h, 'se', idx, 'handle-point');
        this.addSvgHandle(x + w / 2, y + h / 2, 'move', idx, 'handle-point start');
      }
    }
  }

  private addSvgHandle(cx: number, cy: number, type: string, index: number, className: string) {
    const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    circle.setAttribute('cx', cx.toString());
    circle.setAttribute('cy', cy.toString());
    circle.setAttribute('r', '6');
    circle.setAttribute('class', className);
    circle.dataset.handleType = type;
    circle.dataset.annotationIndex = index.toString();
    this.handlesLayerEl.appendChild(circle);
  }

  private addSvgLine(x1: number, y1: number, x2: number, y2: number, className: string) {
    const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
    line.setAttribute('x1', x1.toString());
    line.setAttribute('y1', y1.toString());
    line.setAttribute('x2', x2.toString());
    line.setAttribute('y2', y2.toString());
    line.setAttribute('class', className);
    this.handlesLayerEl.appendChild(line);
  }

  private updateBulkInspector() {
    const indices=[...this.selection];
    const annotations=indices.map(i=>this.scene.annotations[i]);
    const types=[...new Set(annotations.map(a=>a.type))];
    this.inspectorEl.innerHTML='<p>'+indices.length+' 個のマークを選択中</p>';
    if (types.length>1) {
      this.inspectorEl.insertAdjacentHTML('beforeend','<p>同じ種類を選ぶと一括変更できます。</p>');
      for(const type of types) {
        const button=document.createElement('button'); button.className='form-control'; button.textContent=type+' のみ選択';
        button.onclick=()=>this.selectMany(indices.filter(i=>this.scene.annotations[i].type===type)); this.inspectorEl.appendChild(button);
      }
      return;
    }
    const fields: [string,string,string[] | null][]=[['style','スタイル',['primary','secondary','warning','danger','info','step']]];
    if (!['spotlight', 'divider'].includes(types[0])) fields.push(['shadow','影',['true','false']]);
    if (['arrow','bezier-arrow','callout','pin','label'].includes(types[0])) fields.push(['outline','縁取り',['true','false']]);
    if (['badge','step-arrow'].includes(types[0])) fields.push(['step','手順番号',null]);
    if (types[0] === 'rounded-rect') fields.push(['rx','角丸の横半径',null],['ry','角丸の縦半径',null]);
    if (['callout','pin','badge','step-arrow','label','bezier-arrow'].includes(types[0])) fields.push(['position','配置',['auto','top','bottom','left','right','center']]);
    if (types[0] === 'divider') fields.push(['position','配置',['top','bottom','left','right']]);
    if (['arrow','bezier-arrow','step-arrow','rect','rounded-rect','circle','divider'].includes(types[0])) fields.push(['stroke_width','線の太さ',null]);
    if (['arrow','bezier-arrow'].includes(types[0])) fields.push(['line_style','線種',['solid','dashed','dotted']],['arrowhead','矢印の先端',['filled','open']],['arrow_skin','矢印の形',['classic','sketch','bold']],['box','テキスト枠',['true','false']],['text_placement','テキスト位置',['middle','end']]);
    if (['arrow','bezier-arrow','callout','pin','badge','step-arrow','label'].includes(types[0])) fields.push(['text','テキスト',null],['font_family','フォント',['sans-serif','serif','monospace']]);
    if (['arrow','bezier-arrow','callout','label'].includes(types[0])) fields.push(['font_size','文字サイズ',null]);
    for (const [key,title,options] of fields) {
      const label=document.createElement('label'); label.textContent=title;
      const input=document.createElement(options ? 'select' : 'input'); input.className='form-control';
      input.id = `bulk-${key}`;
      label.htmlFor = input.id;
      const values=annotations.map(a=>(a as any)[key]); const common=values.every(v=>v===values[0]) ? values[0] : undefined;
      if(input instanceof HTMLSelectElement) {
        input.add(new Option('混在・未設定（変更しない）',''));
        for(const value of options!) input.add(new Option(value,value));
        input.value=common===undefined ? '' : String(common);
      } else {
        input.type=['stroke_width','font_size','step','rx','ry'].includes(key) ? 'number' : 'text';
        if(input.type==='number') {
          input.min=['rx','ry'].includes(key) ? '0' : '1';
          if (key==='stroke_width') input.max='16';
          if (key==='font_size') input.max='200';
          input.step=key==='step' ? '1' : '0.5';
        }
        input.value=common===undefined ? '' : String(common); input.placeholder='混在・未設定';
      }
      input.addEventListener('change',()=>{
        // Undo or a selection change can blur a pending input while replacing
        // the inspector. Do not let its old handler rewrite the new scene.
        if (annotations.some((a, i) => this.scene.annotations[indices[i]] !== a || !this.selection.has(indices[i]))) return;
        if(input instanceof HTMLSelectElement && input.value==='') return;
        if(input instanceof HTMLInputElement && input.type==='number' && (!input.validity.valid || input.value==='')) return;
        const value=['shadow','box','outline'].includes(key) ? input.value==='true' : ['stroke_width','font_size','step','rx','ry'].includes(key) ? Number(input.value) : input.value;
        for(const a of annotations) {
          (a as any)[key]=value;
          if(key==='text' && String(value).trim()) (a as any).has_text=true;
          if(key==='text_placement') { delete (a as any).text_position; delete (a as any).t; }
        }
        this.pushState(); this.scheduleRender();
      });
      this.inspectorEl.append(label,input);
    }
  }

  private updateInspector() {
    this.updateLayerList();
    if (this.selection.size>1) { this.updateBulkInspector(); return; }
    if (this.selectedIndex === null || this.selectedIndex >= this.scene.annotations.length) {
      this.inspectorEl.innerHTML = '<div class="empty-selection">マークを選択してください</div>';
      return;
    }

    const idx = this.selectedIndex;
    const anno = this.scene.annotations[idx];

    let html = `
      <div class="form-group">
        <label>種類</label>
        <input class="form-control" type="text" value="${anno.type}" disabled />
      </div>

      <div class="form-group">
        <label>スタイル (テーマカラー)</label>
        <div class="style-palette">
          ${['primary', 'secondary', 'warning', 'danger', 'info', 'step']
            .map(
              (st) =>
                `<div class="style-chip ${st} ${anno.style === st ? 'selected' : ''}" data-style="${st}">${st}</div>`
            )
            .join('')}
        </div>
      </div>
    `;

    html += `
      <div class="form-group">
        <label>マークのスタイル</label>
        <div class="stroke-presets">
          <button type="button" class="mark-style-preset" data-mark-style="standard">標準</button>
          <button type="button" class="mark-style-preset" data-mark-style="bold">強調</button>
          <button type="button" class="mark-style-preset" data-mark-style="subtle">控えめ</button>
          <button type="button" class="mark-style-preset" data-mark-style="guide">手順</button>
        </div>
      </div>
    `;

    // Stroke width (線の太さ) controls for arrows and shapes
    const supportsStrokeWidth = ['arrow', 'bezier-arrow', 'step-arrow', 'rect', 'rounded-rect', 'circle', 'divider'].includes(anno.type);
    if (supportsStrokeWidth) {
      const currentStroke = (anno as any).stroke_width ?? (anno.type === 'arrow' || anno.type === 'bezier-arrow' ? 4 : 2.5);
      html += `
        <div class="form-group">
          <label>線の太さ (Stroke Width): <span id="stroke-width-val">${currentStroke}px</span></label>
          <div class="stroke-presets">
            <button type="button" class="stroke-preset-btn ${currentStroke === 1.5 ? 'active' : ''}" data-stroke="1.5">細い (1.5px)</button>
            <button type="button" class="stroke-preset-btn ${currentStroke === 2.5 ? 'active' : ''}" data-stroke="2.5">標準 (2.5px)</button>
            <button type="button" class="stroke-preset-btn ${currentStroke === 4.5 ? 'active' : ''}" data-stroke="4.5">太い (4.5px)</button>
            <button type="button" class="stroke-preset-btn ${currentStroke === 7 ? 'active' : ''}" data-stroke="7">特太 (7px)</button>
          </div>
          <input id="prop-stroke-width" type="range" class="form-control" min="1" max="16" step="0.5" value="${currentStroke}" style="margin-top: 6px;" />
        </div>
      `;
    }

    // Text field & toggle
    const isArrowType = anno.type === 'arrow' || anno.type === 'bezier-arrow';
    if (isArrowType) {
      const hasText = (anno as any).has_text !== undefined ? Boolean((anno as any).has_text) : Boolean((anno as any).text && String((anno as any).text).trim().length > 0);
      const textVal = (anno as any).text ?? 'クリック';
      const arrowSkin = (anno as any).arrow_skin ?? 'classic';
      html += `
        <div class="form-group">
          <label>矢印の形</label>
          <div class="arrow-skin-options" role="group" aria-label="矢印の形">
            <button type="button" class="arrow-skin-option ${arrowSkin === 'classic' ? 'active' : ''}" data-arrow-skin="classic" aria-pressed="${arrowSkin === 'classic'}" title="標準">
              <svg viewBox="0 0 80 30" aria-hidden="true"><path d="M 7 15 H 57" stroke="currentColor" stroke-width="4" stroke-linecap="round"/><path d="M 55 5 L 74 15 L 55 25 Z" fill="currentColor"/></svg><span>標準</span>
            </button>
            <button type="button" class="arrow-skin-option ${arrowSkin === 'sketch' ? 'active' : ''}" data-arrow-skin="sketch" aria-pressed="${arrowSkin === 'sketch'}" title="手描き風">
              <svg viewBox="0 0 80 30" aria-hidden="true"><defs><pattern id="arrow-skin-preview-hatch" width="7" height="7" patternUnits="userSpaceOnUse" patternTransform="rotate(35)"><path d="M 0 0 V 7" stroke="currentColor" stroke-width="1.3"/></pattern></defs><path d="M 5 11 L 53 11 L 53 3 L 75 15 L 53 27 L 53 19 L 5 19 Z" fill="url(#arrow-skin-preview-hatch)" stroke="currentColor" stroke-width="2.5" stroke-linejoin="round"/></svg><span>手描き風</span>
            </button>
            <button type="button" class="arrow-skin-option ${arrowSkin === 'bold' ? 'active' : ''}" data-arrow-skin="bold" aria-pressed="${arrowSkin === 'bold'}" title="先細り">
              <svg viewBox="0 0 80 30" aria-hidden="true"><path d="M 5 15 L 55 11 L 55 3 L 75 15 L 55 27 L 55 19 Z" fill="currentColor"/></svg><span>先細り</span>
            </button>
          </div>
        </div>
        <div class="form-group">
          <label>標準の線</label>
          <select id="prop-arrow-line-style" class="form-control" ${arrowSkin !== 'classic' ? 'disabled' : ''}>
            <option value="solid" ${(anno as any).line_style === undefined || (anno as any).line_style === 'solid' ? 'selected' : ''}>実線</option>
            <option value="dashed" ${(anno as any).line_style === 'dashed' ? 'selected' : ''}>破線</option>
            <option value="dotted" ${(anno as any).line_style === 'dotted' ? 'selected' : ''}>点線</option>
          </select>
          <label>標準の先端</label>
          <select id="prop-arrowhead" class="form-control" ${arrowSkin !== 'classic' ? 'disabled' : ''}>
            <option value="filled" ${(anno as any).arrowhead === undefined || (anno as any).arrowhead === 'filled' ? 'selected' : ''}>塗りつぶし</option>
            <option value="open" ${(anno as any).arrowhead === 'open' ? 'selected' : ''}>輪郭のみ</option>
          </select>
        </div>
        <div class="form-group">
          <label class="checkbox-label" style="display: flex; align-items: center; gap: 8px; cursor: pointer; font-weight: 600; margin-bottom: 6px;">
            <input type="checkbox" id="prop-has-text" ${hasText ? 'checked' : ''} />
            <span>テキストラベルを表示する</span>
          </label>
          <div id="text-edit-container" style="display: ${hasText ? 'block' : 'none'};">
            <label style="font-size: 12px; color: var(--text-muted); margin-bottom: 4px; display: block;">テキスト内容</label>
            <textarea id="prop-text" class="form-control" placeholder="矢印に添えるテキストを入力...">${textVal}</textarea>
            
            <div class="form-group" style="margin-top: 8px; margin-bottom: 8px;">
              <label style="font-size: 12px; font-weight: 600; color: var(--text-color); margin-bottom: 4px; display: block;">ラベルの位置</label>
              <select id="prop-arrow-text-placement" class="form-control">
                <option value="middle" ${((anno as any).text_placement ?? (anno as any).text_position ?? 'middle') === 'middle' ? 'selected' : ''}>中央</option>
                <option value="end" ${((anno as any).text_placement ?? (anno as any).text_position) === 'end' ? 'selected' : ''}>矢印の終端</option>
              </select>
              <div class="arrow-placement-hint">ラベルの大きさに合わせて、矢印の線や終端と重ならない位置に置きます。</div>
            </div>

            <label class="checkbox-label" style="display: flex; align-items: center; gap: 6px; cursor: pointer; margin-top: 8px; font-size: 12px; color: var(--text-muted);">
              <input type="checkbox" id="prop-box" ${(anno as any).box !== false ? 'checked' : ''} />
              <span>テキストに背景枠 (ピルボックス) を付ける</span>
            </label>
          </div>
        </div>
      `;
    } else if ('text' in anno || ['callout', 'label', 'pin'].includes(anno.type)) {
      html += `
        <div class="form-group">
          <label>テキスト内容</label>
          <textarea id="prop-text" class="form-control">${(anno as any).text ?? ''}</textarea>
        </div>
      `;
    }

    // Step number (only for step-arrow and badge)
    if (anno.type === 'badge' || anno.type === 'step-arrow' || ('step' in anno && anno.type !== 'arrow')) {
      html += `
        <div class="form-group">
          <label for="prop-step">手順番号</label>
          <input id="prop-step" class="form-control" type="number" min="1" step="1" value="${Number.isSafeInteger(Number((anno as any).step)) && Number((anno as any).step) > 0 ? Number((anno as any).step) : 1}" />
        </div>
      `;
    }

    // Typography (Font family & Font size)
    const showTypography = anno.type === 'callout' || anno.type === 'label' ||
      ((anno.type === 'bezier-arrow' || anno.type === 'arrow') && ((anno as any).has_text ?? Boolean((anno as any).text)));
    if (showTypography) {
      const currentFamily = (anno as any).font_family ?? 'Arial';
      const currentSize = (anno as any).font_size ?? 14;

      html += `
        <div class="form-group">
          <label>フォント設定</label>
          <select id="prop-font-family" class="form-control">
            <option value="Arial" ${currentFamily === 'Arial' ? 'selected' : ''}>Arial</option>
            <option value="Noto Sans" ${currentFamily === 'Noto Sans' ? 'selected' : ''}>Noto Sans</option>
            <option value="DejaVu Sans" ${currentFamily === 'DejaVu Sans' ? 'selected' : ''}>DejaVu Sans</option>
            <option value="Helvetica" ${currentFamily === 'Helvetica' ? 'selected' : ''}>Helvetica</option>
            <option value="Liberation Sans" ${currentFamily === 'Liberation Sans' ? 'selected' : ''}>Liberation Sans</option>
            <option value="sans-serif" ${currentFamily === 'sans-serif' ? 'selected' : ''}>標準 Sans-Serif</option>
          </select>
        </div>
        <div class="form-group">
          <label>フォントサイズ (px): <span id="font-size-val">${currentSize}</span></label>
          <input id="prop-font-size" type="range" class="form-control" min="10" max="36" value="${currentSize}" />
        </div>
      `;
    }

    // Position Hint affects the text around curved arrows and the placement of other anchored marks.
    const hasPositionHint = anno.type === 'bezier-arrow'
      ? Boolean((anno as any).has_text ?? ((anno as any).text && String((anno as any).text).trim().length > 0))
      : !isArrowType && ('position' in anno || anno.type === 'badge' || anno.type === 'step-arrow' || anno.type === 'label');
    if (hasPositionHint) {
      const pos = (anno as any).position ?? 'auto';
      const isBezierArrow = anno.type === 'bezier-arrow';
      html += `
        <div class="form-group">
          <label for="prop-position">${isBezierArrow ? '曲線に対する文字位置' : anno.type === 'badge' ? 'バッジの配置位置' : '配置位置'}</label>
          <select id="prop-position" class="form-control">
            <option value="auto" ${pos === 'auto' ? 'selected' : ''}>${isBezierArrow ? '曲線の外側（自動）' : '自動'}</option>
            <option value="top" ${pos === 'top' ? 'selected' : ''}>${isBezierArrow ? '曲線の上' : '上'}</option>
            <option value="bottom" ${pos === 'bottom' ? 'selected' : ''}>${isBezierArrow ? '曲線の下' : '下'}</option>
            <option value="left" ${pos === 'left' ? 'selected' : ''}>左</option>
            <option value="right" ${pos === 'right' ? 'selected' : ''}>右</option>
            ${isBezierArrow ? `<option value="center" ${pos === 'center' ? 'selected' : ''}>曲線上（中央）</option>` : ''}
          </select>
          ${isBezierArrow ? '<div class="arrow-placement-hint">文字の置き場所を変えます。矢印の向きや位置は変わりません。</div>' : ''}
        </div>
      `;
    }

    if (anno.type === 'badge' || anno.type === 'label') {
      html += `<div class="form-group">
        <label for="prop-badge-x">対象位置 X (px)</label>
        <input id="prop-badge-x" class="form-control" type="number" step="1" value="${anno.target[0]}" />
        <label for="prop-badge-y">対象位置 Y (px)</label>
        <input id="prop-badge-y" class="form-control" type="number" step="1" value="${anno.target[1]}" />
        <div class="arrow-placement-hint">対象位置と配置方向で位置を調整できます。表示されたマークをドラッグすると、対象のUI部品にスナップします。</div>
      </div>`;
    }

    // Delete Button
    html += `
      <div style="margin-top: 16px;">
        <button id="btn-delete-anno" class="btn btn-danger btn-sm" style="width: 100%;">
          🗑️ このマークを削除 (Del)
        </button>
      </div>
    `;

    this.inspectorEl.innerHTML = html;

    // Bind inspector events
    this.inspectorEl.querySelectorAll('.style-chip').forEach((chip) => {
      chip.addEventListener('click', () => {
        const style = chip.getAttribute('data-style') as SemanticStyle;
        anno.style = style;
        this.inspectorEl.querySelectorAll('.style-chip').forEach((c) => {
          c.classList.toggle('selected', c === chip);
        });
        this.pushState();
        this.scheduleRender();
      });
    });

    const markStylePresets: Record<string, { style: SemanticStyle; stroke: number; shadow: boolean; outline: boolean }> = {
      standard: { style: 'primary', stroke: 2.5, shadow: false, outline: true },
      bold: { style: 'danger', stroke: 7, shadow: true, outline: true },
      subtle: { style: 'secondary', stroke: 1.5, shadow: false, outline: false },
      guide: { style: 'step', stroke: 4.5, shadow: true, outline: true },
    };
    const presetSupportsStroke = ['arrow', 'bezier-arrow', 'step-arrow', 'rect', 'rounded-rect', 'circle'].includes(anno.type);
    const presetSupportsShadow = !['spotlight', 'divider'].includes(anno.type);
    const presetSupportsOutline = ['arrow', 'bezier-arrow', 'callout', 'label', 'pin'].includes(anno.type);
    this.inspectorEl.querySelectorAll('.mark-style-preset').forEach((button) => {
      const key = button.getAttribute('data-mark-style') ?? '';
      const preset = markStylePresets[key];
      if (!preset) return;
      if (anno.style === preset.style && (!presetSupportsStroke || (anno as any).stroke_width === preset.stroke) &&
          (!presetSupportsShadow || anno.shadow === preset.shadow)) button.classList.add('active');
      button.addEventListener('click', () => {
        anno.style = preset.style;
        if (presetSupportsStroke) (anno as any).stroke_width = preset.stroke;
        if (presetSupportsShadow) anno.shadow = preset.shadow;
        if (presetSupportsOutline) (anno as any).outline = preset.outline;
        this.pushState();
        this.scheduleRender();
        this.updateInspector();
      });
    });

    // Stroke width presets
    this.inspectorEl.querySelectorAll('.stroke-preset-btn').forEach((btn) => {
      btn.addEventListener('click', () => {
        const val = parseFloat(btn.getAttribute('data-stroke') || '2.5');
        (anno as any).stroke_width = val;
        const strokeWidthEl = document.getElementById('prop-stroke-width') as HTMLInputElement | null;
        if (strokeWidthEl) strokeWidthEl.value = val.toString();
        const valLabel = document.getElementById('stroke-width-val');
        if (valLabel) valLabel.textContent = `${val}px`;
        this.inspectorEl.querySelectorAll('.stroke-preset-btn').forEach((b) => {
          b.classList.toggle('active', b === btn);
        });
        this.pushState();
        this.scheduleRender();
      });
    });

    // Stroke width slider
    const strokeWidthEl = document.getElementById('prop-stroke-width') as HTMLInputElement | null;
    if (strokeWidthEl) {
      strokeWidthEl.addEventListener('input', () => {
        const val = parseFloat(strokeWidthEl.value);
        (anno as any).stroke_width = val;
        const valLabel = document.getElementById('stroke-width-val');
        if (valLabel) valLabel.textContent = `${val}px`;
        this.inspectorEl.querySelectorAll('.stroke-preset-btn').forEach((btn) => {
          const btnVal = parseFloat(btn.getAttribute('data-stroke') || '0');
          btn.classList.toggle('active', Math.abs(btnVal - val) < 0.05);
        });
        this.scheduleRender();
      });
      strokeWidthEl.addEventListener('change', () => this.pushState());
    }

    // Text toggle checkbox for arrows
    const hasTextCheckbox = document.getElementById('prop-has-text') as HTMLInputElement | null;
    this.inspectorEl.querySelectorAll<HTMLButtonElement>('.arrow-skin-option').forEach((button) => {
      button.addEventListener('click', () => {
        (anno as any).arrow_skin = button.dataset.arrowSkin;
        this.pushState();
        this.updateInspector();
        this.scheduleRender();
      });
    });
    for (const [id, key] of [['prop-arrow-line-style', 'line_style'], ['prop-arrowhead', 'arrowhead']] as const) {
      const select = document.getElementById(id) as HTMLSelectElement | null;
      select?.addEventListener('change', () => {
        (anno as any)[key] = select.value;
        this.pushState();
        this.scheduleRender();
      });
    }
    const textEditContainer = document.getElementById('text-edit-container') as HTMLElement | null;
    if (hasTextCheckbox) {
      hasTextCheckbox.addEventListener('change', () => {
        const checked = hasTextCheckbox.checked;
        (anno as any).has_text = checked;
        if (textEditContainer) {
          textEditContainer.style.display = checked ? 'block' : 'none';
        }
        if (checked && (!(anno as any).text || String((anno as any).text).trim() === '')) {
          (anno as any).text = 'クリック';
          const tEl = document.getElementById('prop-text') as HTMLTextAreaElement | null;
          if (tEl) tEl.value = (anno as any).text;
        }
        this.pushState();
        this.scheduleRender();
        this.updateInspector();
      });
    }

    // Box toggle for arrow and bezier-arrow
    const boxCheckbox = document.getElementById('prop-box') as HTMLInputElement | null;
    if (boxCheckbox) {
      boxCheckbox.addEventListener('change', () => {
        (anno as any).box = boxCheckbox.checked;
        this.pushState();
        this.scheduleRender();
      });
    }

    // Text placement for arrow and bezier-arrow (中間 or 終点)
    const placementSelect = document.getElementById('prop-arrow-text-placement') as HTMLSelectElement | null;
    if (placementSelect) {
      placementSelect.addEventListener('change', () => {
        const val = placementSelect.value as 'middle' | 'end';
        (anno as any).text_placement = val;
        delete (anno as any).text_position;
        if (anno.type === 'bezier-arrow') delete (anno as any).t;
        this.pushState();
        this.scheduleRender();
      });
    }

    const textEl = document.getElementById('prop-text') as HTMLTextAreaElement | null;
    if (textEl) {
      textEl.addEventListener('input', () => {
        (anno as any).text = textEl.value;
        if (textEl.value.trim().length > 0) {
          (anno as any).has_text = true;
        }
        this.scheduleRender();
      });
      textEl.addEventListener('change', () => this.pushState());
    }

    const stepEl = document.getElementById('prop-step') as HTMLInputElement | null;
    if (stepEl) {
      stepEl.addEventListener('input', () => {
        if (!stepEl.validity.valid || !Number.isSafeInteger(stepEl.valueAsNumber)) return;
        (anno as any).step = stepEl.valueAsNumber;
        this.scheduleRender();
      });
      stepEl.addEventListener('change', () => {
        stepEl.value = String((anno as any).step ?? 1);
        this.pushState();
      });
    }

    if (anno.type === 'badge' || anno.type === 'label') {
      for (const [id, coordinate] of [['prop-badge-x', 0], ['prop-badge-y', 1]] as const) {
        const input = document.getElementById(id) as HTMLInputElement;
        input.addEventListener('input', () => {
          if (!Number.isFinite(input.valueAsNumber)) return;
          anno.target[coordinate] = Math.round(input.valueAsNumber);
          this.updateHandles();
          this.scheduleRender();
        });
        input.addEventListener('change', () => { input.value = String(anno.target[coordinate]); this.pushState(); });
      }
    }

    const fontFamEl = document.getElementById('prop-font-family') as HTMLSelectElement | null;
    if (fontFamEl) {
      fontFamEl.addEventListener('change', () => {
        (anno as any).font_family = fontFamEl.value;
        this.pushState();
        this.scheduleRender();
      });
    }

    const fontSizeEl = document.getElementById('prop-font-size') as HTMLInputElement | null;
    if (fontSizeEl) {
      fontSizeEl.addEventListener('input', () => {
        const val = parseInt(fontSizeEl.value, 10);
        (anno as any).font_size = val;
        const valLabel = document.getElementById('font-size-val');
        if (valLabel) valLabel.textContent = val.toString();
        this.scheduleRender();
      });
      fontSizeEl.addEventListener('change', () => this.pushState());
    }

    const posEl = document.getElementById('prop-position') as HTMLSelectElement | null;
    if (posEl) {
      posEl.addEventListener('change', () => {
        (anno as any).position = posEl.value as PositionHint;
        this.pushState();
        this.scheduleRender();
      });
    }

    document.getElementById('btn-delete-anno')?.addEventListener('click', () => {
      this.removeAnnotation(idx);
    });
  }

  // --- Crop Mode Methods ---

  public toggleCropMode(): boolean {
    if (this.isCropMode) {
      this.cancelCropMode();
      return false;
    } else {
      this.startCropMode();
      return true;
    }
  }

  public startCropMode(): void {
    if (!this.hasImage()) return;
    this.isCropMode = true;
    this.selectedIndex = null;
    this.updateHandles();
    this.updateInspector();
    if (this.snapGuideLayerEl) this.snapGuideLayerEl.innerHTML = '';

    const cw = this.scene.canvas.width;
    const ch = this.scene.canvas.height;
    this.cropRect = { x: 0, y: 0, width: cw, height: ch };
    if (this.cropPresetSelectEl) {
      this.cropPresetSelectEl.value = 'free';
    }
    this.cropPresetRatio = null;
    this.updateCropOverlayUi();
    if (this.editorCropOverlayEl) {
      this.editorCropOverlayEl.style.display = 'block';
    }

    const btnCrop = document.getElementById('btn-crop');
    if (btnCrop) btnCrop.classList.add('active');
  }

  public cancelCropMode(): void {
    this.isCropMode = false;
    this.isDraggingCrop = false;
    this.activeCropHandle = null;
    this.cropRect = null;
    if (this.cropPresetSelectEl) {
      this.cropPresetSelectEl.value = 'free';
    }
    this.cropPresetRatio = null;
    if (this.editorCropOverlayEl) {
      this.editorCropOverlayEl.style.display = 'none';
    }

    const btnCrop = document.getElementById('btn-crop');
    if (btnCrop) btnCrop.classList.remove('active');

    this.renderAllSnapTargets();
    this.updateHandles();
  }

  public resetCrop(): void {
    if (!this.isCropMode) return;
    if (this.cropPresetSelectEl) {
      this.cropPresetSelectEl.value = 'free';
    }
    this.cropPresetRatio = null;
    const cw = this.scene.canvas.width;
    const ch = this.scene.canvas.height;
    this.cropRect = { x: 0, y: 0, width: cw, height: ch };
    this.updateCropOverlayUi();
  }

  public getCropRect(): { x: number; y: number; width: number; height: number } | null {
    return this.cropRect ? { ...this.cropRect } : null;
  }

  public applyCropPreset(preset: string): void {
    if (!this.isCropMode || !this.hasImage()) return;
    if (preset === 'free') {
      this.cropPresetRatio = null;
      return;
    }
    const parts = preset.split('x').map(Number);
    if (parts.length !== 2 || isNaN(parts[0]) || isNaN(parts[1])) return;
    const [targetW, targetH] = parts;
    if (targetW <= 0 || targetH <= 0) return;

    this.cropPresetRatio = targetW / targetH;

    const cw = this.scene.canvas.width;
    const ch = this.scene.canvas.height;

    let w = targetW;
    let h = targetH;

    // If preset dimensions exceed canvas, fit within canvas while preserving ratio
    if (w > cw || h > ch) {
      const ratio = targetW / targetH;
      if (cw / ch > ratio) {
        h = ch;
        w = Math.round(h * ratio);
      } else {
        w = cw;
        h = Math.round(w / ratio);
      }
    }

    // Keep current center if cropRect exists, otherwise center on canvas
    const currentCenterX = this.cropRect ? this.cropRect.x + this.cropRect.width / 2 : cw / 2;
    const currentCenterY = this.cropRect ? this.cropRect.y + this.cropRect.height / 2 : ch / 2;

    let nx = Math.round(currentCenterX - w / 2);
    let ny = Math.round(currentCenterY - h / 2);
    nx = Math.max(0, Math.min(cw - w, nx));
    ny = Math.max(0, Math.min(ch - h, ny));

    this.cropRect = {
      x: nx,
      y: ny,
      width: w,
      height: h,
    };
    this.updateCropOverlayUi();
  }

  public applyCrop(): void {
    if (!this.cropRect || this.cropRect.width < 10 || this.cropRect.height < 10) {
      this.cancelCropMode();
      return;
    }

    const rx = Math.round(this.cropRect.x);
    const ry = Math.round(this.cropRect.y);
    const rw = Math.round(this.cropRect.width);
    const rh = Math.round(this.cropRect.height);

    if (rw <= 0 || rh <= 0) {
      this.cancelCropMode();
      return;
    }

    // Crop image via HTML5 Canvas
    const canvas = document.createElement('canvas');
    canvas.width = rw;
    canvas.height = rh;
    const ctx = canvas.getContext('2d');
    if (!ctx) {
      this.cancelCropMode();
      return;
    }

    ctx.drawImage(this.bgImgEl, rx, ry, rw, rh, 0, 0, rw, rh);
    const croppedDataUrl = canvas.toDataURL('image/png');

    // Shift existing annotations
    const shiftedAnnotations = this.scene.annotations
      .map((anno) => {
        const clone = JSON.parse(JSON.stringify(anno));
        if (clone.type === 'arrow') {
          if (clone.start) { clone.start[0] -= rx; clone.start[1] -= ry; }
          if (clone.end) { clone.end[0] -= rx; clone.end[1] -= ry; }
        } else if (clone.type === 'bezier-arrow') {
          clone.start[0] -= rx; clone.start[1] -= ry;
          clone.control[0] -= rx; clone.control[1] -= ry;
          clone.end[0] -= rx; clone.end[1] -= ry;
        } else if ('target' in clone && Array.isArray(clone.target)) {
          clone.target[0] -= rx;
          clone.target[1] -= ry;
        }
        return clone;
      });

    // Shift detected UI elements and keep those intersecting with crop region
    const shiftedUiElements: DetectedUiElement[] = [];
    for (const el of this.uiElements) {
      const ex = el.x - rx;
      const ey = el.y - ry;
      if (ex + el.width > 0 && ex < rw && ey + el.height > 0 && ey < rh) {
        shiftedUiElements.push({
          ...el,
          x: ex,
          y: ey,
        });
      }
    }

    if (this.isAutoCropped && this.baseImageState) {
      this.cropHistoryStack = [{ dataUrl: this.baseImageState.dataUrl, width: this.baseImageState.width, height: this.baseImageState.height, uiElements: this.baseImageState.uiElements.map(el => ({ ...el })), offset: { ...this.autoCropOffset }, annotations: [] }];
      this.isAutoCropped = false;
    }
    this.cropHistoryStack.push({
      dataUrl: this.bgImgEl.src,
      width: this.scene.canvas.width,
      height: this.scene.canvas.height,
      uiElements: this.uiElements.map((el) => ({ ...el })),
      offset: { x: rx, y: ry },
      annotations: JSON.parse(JSON.stringify(this.scene.annotations)),
    });

    this.cancelCropMode();
    this.setBackgroundImage(
      croppedDataUrl,
      rw,
      rh,
      JSON.stringify({ canvas: { width: rw, height: rh }, shadow: this.scene.shadow, annotations: shiftedAnnotations }),
      shiftedUiElements,
      true
    );
    this.updateRevertCropButton();
    window.dispatchEvent(new CustomEvent('markits-crop-changed'));
  }

  public toggleAutoCrop(margin: number = 32): boolean {
    if (!this.hasImage()) return false;

    if (this.isAutoCropped) {
      if (!this.baseImageState) return false;

      const unshiftedAnnotations = this.scene.annotations.map((anno) => {
        const clone = JSON.parse(JSON.stringify(anno));
        if (clone.type === 'arrow') {
          if (clone.start) { clone.start[0] += this.autoCropOffset.x; clone.start[1] += this.autoCropOffset.y; }
          if (clone.end) { clone.end[0] += this.autoCropOffset.x; clone.end[1] += this.autoCropOffset.y; }
        } else if (clone.type === 'bezier-arrow') {
          clone.start[0] += this.autoCropOffset.x; clone.start[1] += this.autoCropOffset.y;
          clone.control[0] += this.autoCropOffset.x; clone.control[1] += this.autoCropOffset.y;
          clone.end[0] += this.autoCropOffset.x; clone.end[1] += this.autoCropOffset.y;
        } else if ('target' in clone && Array.isArray(clone.target)) {
          clone.target[0] += this.autoCropOffset.x;
          clone.target[1] += this.autoCropOffset.y;
        }
        return clone;
      });

      this.isAutoCropped = false;
      this.autoCropOffset = { x: 0, y: 0 };
      const btnAutoCrop = document.getElementById('btn-autocrop');
      if (btnAutoCrop) btnAutoCrop.classList.remove('active');

      this.setBackgroundImage(
        this.baseImageState.dataUrl,
        this.baseImageState.width,
        this.baseImageState.height,
        JSON.stringify({
          canvas: { width: this.baseImageState.width, height: this.baseImageState.height },
          shadow: this.scene.shadow,
          annotations: unshiftedAnnotations,
        }),
        this.baseImageState.uiElements.map((el) => ({ ...el })),
        true
      );
      this.updateRevertCropButton();
      window.dispatchEvent(new CustomEvent('markits-crop-changed'));
      return false;
    } else {
      const bounds = this.getAnnotationBounds();
      if (!bounds) return false;

      const cw = this.scene.canvas.width;
      const ch = this.scene.canvas.height;
      const rx = Math.max(0, Math.floor(bounds.minX - margin));
      const ry = Math.max(0, Math.floor(bounds.minY - margin));
      const rw = Math.min(cw - rx, Math.ceil(bounds.maxX + margin) - rx);
      const rh = Math.min(ch - ry, Math.ceil(bounds.maxY + margin) - ry);

      if (rw <= 0 || rh <= 0) return false;

      const canvas = document.createElement('canvas');
      canvas.width = rw;
      canvas.height = rh;
      const ctx = canvas.getContext('2d');
      if (!ctx) return false;

      ctx.drawImage(this.bgImgEl, rx, ry, rw, rh, 0, 0, rw, rh);
      const croppedDataUrl = canvas.toDataURL('image/png');

      const shiftedAnnotations = this.scene.annotations.map((anno) => {
        const clone = JSON.parse(JSON.stringify(anno));
        if (clone.type === 'arrow') {
          if (clone.start) { clone.start[0] -= rx; clone.start[1] -= ry; }
          if (clone.end) { clone.end[0] -= rx; clone.end[1] -= ry; }
        } else if (clone.type === 'bezier-arrow') {
          clone.start[0] -= rx; clone.start[1] -= ry;
          clone.control[0] -= rx; clone.control[1] -= ry;
          clone.end[0] -= rx; clone.end[1] -= ry;
        } else if ('target' in clone && Array.isArray(clone.target)) {
          clone.target[0] -= rx;
          clone.target[1] -= ry;
        }
        return clone;
      });

      const shiftedUiElements: DetectedUiElement[] = [];
      for (const el of this.uiElements) {
        const ex = el.x - rx;
        const ey = el.y - ry;
        if (ex + el.width > 0 && ex < rw && ey + el.height > 0 && ey < rh) {
          shiftedUiElements.push({
            ...el,
            x: ex,
            y: ey,
          });
        }
      }

      this.isAutoCropped = true;
      const manualOffset = this.cropHistoryStack.reduce((offset,item) => ({ x:offset.x+item.offset.x,y:offset.y+item.offset.y }), {x:0,y:0});
      this.autoCropOffset = { x: rx + manualOffset.x, y: ry + manualOffset.y };
      const btnAutoCrop = document.getElementById('btn-autocrop');
      if (btnAutoCrop) btnAutoCrop.classList.add('active');

      this.setBackgroundImage(
        croppedDataUrl,
        rw,
        rh,
        JSON.stringify({ canvas: { width: rw, height: rh }, shadow: this.scene.shadow, annotations: shiftedAnnotations }),
        shiftedUiElements,
        true
      );
      this.updateRevertCropButton();
      window.dispatchEvent(new CustomEvent('markits-crop-changed'));
      return true;
    }
  }

  public revertCrop(): void {
    if (!this.baseImageState) return;

    if (this.isAutoCropped) {
      this.toggleAutoCrop();
      return;
    }

    if (this.cropHistoryStack.length > 0) {
      let totalOffsetX = 0;
      let totalOffsetY = 0;
      for (const item of this.cropHistoryStack) {
        totalOffsetX += item.offset.x;
        totalOffsetY += item.offset.y;
      }

      const unshiftedAnnotations = this.scene.annotations.map((anno) => {
        const clone = JSON.parse(JSON.stringify(anno));
        if (clone.type === 'arrow') {
          if (clone.start) { clone.start[0] += totalOffsetX; clone.start[1] += totalOffsetY; }
          if (clone.end) { clone.end[0] += totalOffsetX; clone.end[1] += totalOffsetY; }
        } else if (clone.type === 'bezier-arrow') {
          clone.start[0] += totalOffsetX; clone.start[1] += totalOffsetY;
          clone.control[0] += totalOffsetX; clone.control[1] += totalOffsetY;
          clone.end[0] += totalOffsetX; clone.end[1] += totalOffsetY;
        } else if ('target' in clone && Array.isArray(clone.target)) {
          clone.target[0] += totalOffsetX;
          clone.target[1] += totalOffsetY;
        }
        return clone;
      });

      this.cropHistoryStack = [];
      this.isAutoCropped = false;
      this.autoCropOffset = { x: 0, y: 0 };
      const btnAutoCrop = document.getElementById('btn-autocrop');
      if (btnAutoCrop) btnAutoCrop.classList.remove('active');

      this.setBackgroundImage(
        this.baseImageState.dataUrl,
        this.baseImageState.width,
        this.baseImageState.height,
        JSON.stringify({
          canvas: { width: this.baseImageState.width, height: this.baseImageState.height },
          shadow: this.scene.shadow,
          annotations: unshiftedAnnotations,
        }),
        this.baseImageState.uiElements.map((el) => ({ ...el })),
        true
      );
      this.updateRevertCropButton();
      window.dispatchEvent(new CustomEvent('markits-crop-changed'));
    }
  }

  public getAnnotationBounds(): { minX: number; minY: number; maxX: number; maxY: number } | null {
    if (this.scene.annotations.length === 0) return null;
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    const includePoint = (point: [number, number], padding = 0) => {
      minX = Math.min(minX, point[0] - padding);
      minY = Math.min(minY, point[1] - padding);
      maxX = Math.max(maxX, point[0] + padding);
      maxY = Math.max(maxY, point[1] + padding);
    };

    for (const anno of this.scene.annotations) {
      if (anno.type === 'arrow') {
        const length = anno.start && anno.end ? Math.hypot(anno.end[0] - anno.start[0], anno.end[1] - anno.start[1]) : 0;
        const strokeWidth = (anno as any).stroke_width ?? 4;
        const padding = anno.arrow_skin === 'bold' ? Math.min(65, Math.max(10, length * 0.12)) + 3 : Math.min(Math.max(strokeWidth * 2.3, 10), 22) + 3;
        if (anno.start) includePoint(anno.start, padding);
        if (anno.end) includePoint(anno.end, padding);
      } else if (anno.type === 'bezier-arrow') {
        const length = Math.hypot(anno.control[0] - anno.start[0], anno.control[1] - anno.start[1]) + Math.hypot(anno.end[0] - anno.control[0], anno.end[1] - anno.control[1]);
        const strokeWidth = (anno as any).stroke_width ?? 4;
        const padding = anno.arrow_skin === 'bold' ? Math.min(65, Math.max(10, length * 0.12)) + 3 : Math.min(Math.max(strokeWidth * 2.3, 10), 22) + 3;
        for (const pt of [anno.start, anno.control, anno.end]) {
          if (pt) includePoint(pt, padding);
        }
      } else if ('target' in anno && Array.isArray(anno.target)) {
        const [x, y, w, h] = anno.target;
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x + w);
        maxY = Math.max(maxY, y + h);
      }
    }

    if (!isFinite(minX) || !isFinite(minY) || !isFinite(maxX) || !isFinite(maxY)) {
      return null;
    }
    return { minX, minY, maxX, maxY };
  }

  public fitCropToAnnotations(margin: number = 32): boolean {
    const bounds = this.getAnnotationBounds();
    if (!bounds) return false;

    const cw = this.scene.canvas.width;
    const ch = this.scene.canvas.height;
    const rx = Math.max(0, Math.floor(bounds.minX - margin));
    const ry = Math.max(0, Math.floor(bounds.minY - margin));
    const rw = Math.min(cw - rx, Math.ceil(bounds.maxX + margin) - rx);
    const rh = Math.min(ch - ry, Math.ceil(bounds.maxY + margin) - ry);

    if (rw <= 0 || rh <= 0) return false;

    if (this.cropPresetSelectEl) {
      this.cropPresetSelectEl.value = 'free';
    }
    this.cropPresetRatio = null;

    this.cropRect = { x: rx, y: ry, width: rw, height: rh };
    this.updateCropOverlayUi();
    return true;
  }

  public cropToAnnotations(margin: number = 32): void {
    this.toggleAutoCrop(margin);
  }

  private handleCropMouseDown(e: MouseEvent): void {
    if (e.button !== 0 || !this.isCropMode) return;
    const target = e.target as HTMLElement;

    // Ignore toolbar button clicks inside crop box
    if (target.closest('.crop-toolbar') || target.tagName === 'BUTTON') {
      return;
    }

    const coords = this.getCanvasCoords(e);
    const handle = target.getAttribute('data-crop-handle');

    this.isDraggingCrop = true;
    this.cropDragStartX = coords.x;
    this.cropDragStartY = coords.y;
    this.initialCropRect = this.cropRect
      ? { ...this.cropRect }
      : { x: 0, y: 0, width: this.scene.canvas.width, height: this.scene.canvas.height };

    if (handle) {
      this.activeCropHandle = handle;
    } else if (target.closest('#crop-box')) {
      this.activeCropHandle = 'move';
    } else {
      // Clicked outside crop box: start new crop drag
      this.activeCropHandle = 'new';
      if (this.cropPresetSelectEl) {
        this.cropPresetSelectEl.value = 'free';
      }
      this.cropPresetRatio = null;
      this.cropRect = {
        x: Math.round(coords.x),
        y: Math.round(coords.y),
        width: 0,
        height: 0,
      };
      this.updateCropOverlayUi();
    }

    e.preventDefault();
    e.stopPropagation();
  }

  private handleCropMouseMove(e: MouseEvent): void {
    if (!this.isCropMode || !this.isDraggingCrop || !this.initialCropRect) return;
    const coords = this.getCanvasCoords(e);
    const cw = this.scene.canvas.width;
    const ch = this.scene.canvas.height;
    const dx = coords.x - this.cropDragStartX;
    const dy = coords.y - this.cropDragStartY;
    const init = this.initialCropRect;

    if (this.activeCropHandle === 'move') {
      let nx = init.x + dx;
      let ny = init.y + dy;
      nx = Math.max(0, Math.min(cw - init.width, nx));
      ny = Math.max(0, Math.min(ch - init.height, ny));
      this.cropRect = {
        x: Math.round(nx),
        y: Math.round(ny),
        width: init.width,
        height: init.height,
      };
    } else if (this.activeCropHandle === 'new') {
      const x1 = Math.max(0, Math.min(cw, Math.min(this.cropDragStartX, coords.x)));
      const y1 = Math.max(0, Math.min(ch, Math.min(this.cropDragStartY, coords.y)));
      const x2 = Math.max(0, Math.min(cw, Math.max(this.cropDragStartX, coords.x)));
      const y2 = Math.max(0, Math.min(ch, Math.max(this.cropDragStartY, coords.y)));
      this.cropRect = {
        x: Math.round(x1),
        y: Math.round(y1),
        width: Math.round(x2 - x1),
        height: Math.round(y2 - y1),
      };
    } else if (this.activeCropHandle) {
      let x = init.x;
      let y = init.y;
      let w = init.width;
      let h = init.height;

      const isCorner = ['nw', 'ne', 'se', 'sw'].includes(this.activeCropHandle);
      const ratio = (e.shiftKey ? 1.0 : this.cropPresetRatio);

      if (ratio && isCorner) {
        // Constrained ratio resize from corner
        if (this.activeCropHandle === 'se') {
          w = Math.max(20, Math.min(cw - x, init.width + dx));
          h = Math.round(w / ratio);
          if (y + h > ch) {
            h = ch - y;
            w = Math.round(h * ratio);
          }
        } else if (this.activeCropHandle === 'sw') {
          const right = init.x + init.width;
          w = Math.max(20, Math.min(right, init.width - dx));
          h = Math.round(w / ratio);
          if (y + h > ch) {
            h = ch - y;
            w = Math.round(h * ratio);
          }
          x = right - w;
        } else if (this.activeCropHandle === 'ne') {
          const bottom = init.y + init.height;
          w = Math.max(20, Math.min(cw - init.x, init.width + dx));
          h = Math.round(w / ratio);
          if (bottom - h < 0) {
            h = bottom;
            w = Math.round(h * ratio);
          }
          y = bottom - h;
        } else if (this.activeCropHandle === 'nw') {
          const right = init.x + init.width;
          const bottom = init.y + init.height;
          w = Math.max(20, Math.min(right, init.width - dx));
          h = Math.round(w / ratio);
          if (bottom - h < 0) {
            h = bottom;
            w = Math.round(h * ratio);
          }
          x = right - w;
          y = bottom - h;
        }
      } else {
        // Free resize
        if (this.cropPresetSelectEl && this.cropPresetSelectEl.value !== 'free') {
          this.cropPresetSelectEl.value = 'free';
          this.cropPresetRatio = null;
        }
        // Handle East / West
        if (this.activeCropHandle.includes('e')) {
          w = Math.max(20, Math.min(cw - x, init.width + dx));
        } else if (this.activeCropHandle.includes('w')) {
          const right = init.x + init.width;
          x = Math.max(0, Math.min(right - 20, init.x + dx));
          w = right - x;
        }

        // Handle North / South
        if (this.activeCropHandle.includes('s')) {
          h = Math.max(20, Math.min(ch - y, init.height + dy));
        } else if (this.activeCropHandle.includes('n')) {
          const bottom = init.y + init.height;
          y = Math.max(0, Math.min(bottom - 20, init.y + dy));
          h = bottom - y;
        }
      }

      this.cropRect = {
        x: Math.round(x),
        y: Math.round(y),
        width: Math.round(w),
        height: Math.round(h),
      };
    }

    this.updateCropOverlayUi();
  }

  private handleCropMouseUp(): void {
    this.isDraggingCrop = false;
    this.activeCropHandle = null;
    if (this.cropRect) {
      if (this.cropRect.width < 10 || this.cropRect.height < 10) {
        this.resetCrop();
      }
    }
  }

  private updateCropOverlayUi(): void {
    if (!this.cropRect || !this.cropBoxEl) return;
    const cw = this.scene.canvas.width;
    const ch = this.scene.canvas.height;
    const { x, y, width, height } = this.cropRect;

    this.cropBoxEl.style.left = `${x}px`;
    this.cropBoxEl.style.top = `${y}px`;
    this.cropBoxEl.style.width = `${width}px`;
    this.cropBoxEl.style.height = `${height}px`;

    if (this.cropMaskTop) {
      this.cropMaskTop.style.cssText = `left: 0; top: 0; width: 100%; height: ${Math.max(0, y)}px;`;
    }
    if (this.cropMaskBottom) {
      this.cropMaskBottom.style.cssText = `left: 0; top: ${y + height}px; width: 100%; height: ${Math.max(0, ch - (y + height))}px;`;
    }
    if (this.cropMaskLeft) {
      this.cropMaskLeft.style.cssText = `left: 0; top: ${y}px; width: ${Math.max(0, x)}px; height: ${height}px;`;
    }
    if (this.cropMaskRight) {
      this.cropMaskRight.style.cssText = `left: ${x + width}px; top: ${y}px; width: ${Math.max(0, cw - (x + width))}px; height: ${height}px;`;
    }

    if (this.cropSizeBadgeEl) {
      this.cropSizeBadgeEl.textContent = `${Math.round(width)} × ${Math.round(height)}`;
    }
  }

  // --- Zoom Controls ---

  public setOutputDimensions(width: number, height: number): void {
    if (!Number.isInteger(width) || !Number.isInteger(height) || width < 10 || height < 10
        || width > 10000 || height > 10000) return;
    this.outputWidth = width;
    this.outputHeight = height;
    this.setZoom(this.zoomLevel);
    this.zoomFit();
  }

  public getOutputDimensions(): { width: number; height: number } {
    return { width: this.outputWidth, height: this.outputHeight };
  }

  public setZoom(level: number): void {
    this.zoomLevel = Math.max(0.2, Math.min(5.0, Math.round(level * 100) / 100));
    const sx = this.zoomLevel * this.outputWidth / Math.max(1, this.scene.canvas.width);
    const sy = this.zoomLevel * this.outputHeight / Math.max(1, this.scene.canvas.height);
    this.canvasContainerEl.style.transform = `scale(${sx}, ${sy})`;
    this.canvasContainerEl.style.transformOrigin = 'center center';
    if (this.zoomLabelEl) {
      this.zoomLabelEl.textContent = `${Math.round(this.zoomLevel * 100)}%`;
    }
  }

  public getZoom(): number {
    return this.zoomLevel;
  }

  public zoomIn(): void {
    this.setZoom(this.zoomLevel + 0.15);
  }

  public zoomOut(): void {
    this.setZoom(this.zoomLevel - 0.15);
  }

  public resetZoom(): void {
    this.setZoom(1.0);
  }

  private centerImage(): void {
    const viewport = this.viewportEl.getBoundingClientRect();
    const image = this.canvasContainerEl.getBoundingClientRect();
    this.viewportEl.scrollLeft += image.left + image.width / 2 - viewport.left - viewport.width / 2;
    this.viewportEl.scrollTop += image.top + image.height / 2 - viewport.top - viewport.height / 2;
  }

  public zoomFit(): void {
    const vpWidth = this.viewportEl.clientWidth - 48;
    const vpHeight = this.viewportEl.clientHeight - 48;
    const cw = this.outputWidth;
    const ch = this.outputHeight;
    if (cw <= 0 || ch <= 0 || vpWidth <= 0 || vpHeight <= 0) return;
    const scaleX = vpWidth / cw;
    const scaleY = vpHeight / ch;
    const fitScale = Math.min(scaleX, scaleY, 1.0);
    this.setZoom(fitScale);
    this.centerImage();
  }
}
