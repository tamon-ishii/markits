const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');

function fixture() {
  const modules = new Map();
  class Node {}
  function load(file) {
    if (file.endsWith('.json')) return { default: JSON.parse(fs.readFileSync(file, 'utf8')) };
    if (modules.has(file)) return modules.get(file);
    const exports = {};
    modules.set(file, exports);
    const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), { compilerOptions: {
      module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022,
    } }).outputText;
    vm.runInNewContext(code, { exports, Node, console, navigator: { language: 'ja' },
      require: id => load(path.resolve(path.dirname(file), id)),
    });
    return exports;
  }
  const { AnnotationEditor } = load(path.join(__dirname, '../src/editor.ts'));
  const editor = Object.create(AnnotationEditor.prototype);
  Object.assign(editor, {
    scene: { canvas: { width: 1000, height: 800 }, shadow: true, annotations: [] },
    uiElements: [{ x: 200, y: 200, width: 200, height: 80, name: 'Save', role: 'button' }],
    isSnapEnabled: true, activeTool: 'rect', hoverPreview: null, hoverPreviewKey: '', hoverPreviewRevision: 0,
    canvasContainerEl: { contains: () => true }, svgLayerEl: { innerHTML: '' },
    getCanvasCoords: event => ({ x: event.x, y: event.y }),
    scheduleRender() {}, renderAllSnapTargets() {}, updateHandles() {}, renderMosaicPreview() {},
  });
  const hover = (x = 250, y = 230, altKey = false) => editor.handleMouseMove({ x, y, altKey, target: new Node() });
  return { editor, hover };
}

test('hover inside a candidate previews the same annotation a click will create', () => {
  const { editor, hover } = fixture();
  hover();
  assert.equal(editor.hoverPreview.type, 'rect');
  assert.deepEqual(Array.from(editor.hoverPreview.target), [200, 200, 200, 80]);
  assert.equal(JSON.stringify(editor.hoverPreview), JSON.stringify(editor.createSnappedAnnotation('rect', editor.creationSnap(250, 230, false))));
  assert.equal(editor.scene.annotations.length, 0);
  assert.equal(editor.getCleanScene().annotations.length, 0);
});

test('each tool uses the actual renderer without adding preview to exported scene', async () => {
  for (const tool of ['arrow', 'bezier-arrow', 'badge', 'step-arrow', 'rect', 'rounded-rect', 'circle', 'mosaic', 'spotlight', 'bullseye', 'divider', 'callout', 'pin']) {
    const { editor, hover } = fixture();
    editor.activeTool = tool;
    hover();
    let rendered;
    editor.renderCallback = async json => { rendered = JSON.parse(json); return '<svg>preview</svg>'; };
    await editor.executeRender();
    assert.equal(rendered.annotations.length, 1, tool);
    assert.equal(rendered.annotations[0].type, tool === 'step-arrow' ? 'badge' : tool);
    assert.equal(editor.scene.annotations.length, 0);
    assert.equal(editor.getCleanScene().annotations.length, 0);
  }
});

test('callout target box and handles follow resize, center drag, and body drag without replacing nodes', () => {
  for (const kind of ['se', 'move', 'body']) {
    const { editor } = fixture();
    const node = () => ({ attributes: {}, setAttribute(key, value) { this.attributes[key] = value; } });
    const box = node(), resize = node(), move = node();
    move.dataset = { centerX: '150', centerY: '140' };
    editor.handlesLayerEl = {
      querySelector: selector => selector.startsWith('.handle-bbox') ? box : selector.startsWith('circle') ? resize : move,
    };
    editor.scene.annotations = [{ type: 'callout', target: [100, 100, 100, 80], text: 'Example' }];
    editor.selectedIndex = 0;
    editor.initialAnnotationState = JSON.parse(JSON.stringify(editor.scene.annotations[0]));
    editor.dragStartX = kind === 'se' ? 200 : 150;
    editor.dragStartY = kind === 'se' ? 180 : 140;
    editor.isDraggingHandle = kind !== 'body';
    editor.isDraggingAnnotation = kind === 'body';
    editor.activeHandle = { type: kind, index: 0 };
    editor.handleMouseMove({ x: editor.dragStartX + 30, y: editor.dragStartY + 20, altKey: true });
    assert.deepEqual(box.attributes, kind === 'se'
      ? { x: '100', y: '100', width: '130', height: '100' }
      : { x: '130', y: '120', width: '100', height: '80' });
    assert.deepEqual(resize.attributes, { cx: '230', cy: '200' });
    assert.equal(move.attributes.transform, kind === 'se' ? 'translate(15 10)' : 'translate(30 20)');
    // A second event still updates the same nodes during the active drag.
    editor.handleMouseMove({ x: editor.dragStartX + 40, y: editor.dragStartY + 30, altKey: true });
    assert.deepEqual(resize.attributes, { cx: '240', cy: '210' });
    assert.equal(editor.isDraggingHandle || editor.isDraggingAnnotation, true);
  }
});

test('straight and curved arrows keep endpoints, control point and guide lines on the active drag', () => {
  for (const type of ['arrow', 'bezier-arrow']) {
    for (const kind of type === 'arrow' ? ['start', 'end', 'body'] : ['start', 'control', 'end', 'body']) {
      const { editor } = fixture();
      const nodes = new Map();
      editor.handlesLayerEl = { querySelector(selector) {
        if (!nodes.has(selector)) nodes.set(selector, { attributes: {}, setAttribute(key, value) { this.attributes[key] = value; } });
        return nodes.get(selector);
      } };
      editor.scene.annotations = [{ type, start: [100, 100], end: [300, 200], ...(type === 'bezier-arrow' ? { control: [200, 60] } : {}) }];
      editor.selectedIndex = 0;
      editor.initialAnnotationState = JSON.parse(JSON.stringify(editor.scene.annotations[0]));
      editor.dragStartX = 200; editor.dragStartY = 100;
      editor.isDraggingHandle = kind !== 'body'; editor.isDraggingAnnotation = kind === 'body';
      editor.activeHandle = { type: kind, index: 0 };
      editor.handleMouseMove({ x: 230, y: 120, altKey: true });
      const annotation = editor.scene.annotations[0];
      for (const key of ['start', 'end', ...(type === 'bezier-arrow' ? ['control'] : [])]) {
        const attributes = nodes.get(`circle[data-handle-type="${key}"][data-annotation-index="0"]`).attributes;
        assert.deepEqual(attributes, { cx: String(annotation[key][0]), cy: String(annotation[key][1]) });
      }
      for (const guide of type === 'arrow' ? ['start-end'] : ['start-control', 'control-end']) {
        const [from, to] = guide.split('-').map(key => annotation[key]);
        assert.deepEqual(nodes.get(`line[data-guide="${guide}"][data-annotation-index="0"]`).attributes,
          { x1: String(from[0]), y1: String(from[1]), x2: String(to[0]), y2: String(to[1]) });
      }
    }
  }
});

test('mark copy/paste preserves groups, styles and numbers, offsets repeated pastes, and supports undo', () => {
  const { editor } = fixture();
  Object.assign(editor, { hasImage: () => true, setTool() {}, updateInspector() {}, updateLayerList() {}, render() {},
    undoStack: [], redoStack: [], cropHistoryStack: [] });
  editor.scene.annotations = [
    { type: 'badge', target: [40, 50, 80, 30], step: 3, arrow: true, target_box: true, style: 'danger' },
    { type: 'bezier-arrow', start: [100, 120], control: [150, 70], end: [200, 120], arrow_skin: 'bold', stroke_width: 7 },
    { type: 'rect', target: [20, 20, 30, 40] },
  ];
  editor.selectedIndex = 0; editor.selection = new Set([1, 0, 2]); editor.scene.hidden_annotations = [2];
  editor.pushState();
  const text = editor.copySelectedMarks();
  assert.equal(JSON.parse(text).annotations.length, 2);
  // Copy is a snapshot, independent of later edits to the source mark.
  editor.scene.annotations[0].step = 9;
  assert.equal(editor.pasteMarks(text), true);
  assert.equal(editor.scene.annotations[3].step, 3);
  assert.equal(editor.scene.annotations[3].arrow, true);
  assert.deepEqual(Array.from(editor.scene.annotations[3].target), [60, 70, 80, 30]);
  assert.deepEqual(Array.from(editor.scene.annotations[4].control), [170, 90]);
  assert.deepEqual(Array.from(editor.selection), [3, 4]);
  assert.equal(editor.pasteMarks(text), true);
  assert.deepEqual(Array.from(editor.scene.annotations[5].target), [80, 90, 80, 30]);
  editor.undo(); assert.equal(editor.scene.annotations.length, 5);
  editor.redo(); assert.equal(editor.scene.annotations.length, 7);
  for (const invalid of ['ordinary text', '{}', '{"format":"markits-annotations","version":1,"annotations":[{"type":"rect","target":[0,0,-2,5]}]}']) {
    assert.equal(editor.pasteMarks(invalid), false);
  }
});

test('labels hit only their visible bodies and drag freely without their old targets or UI snapping', () => {
  const { editor } = fixture();
  const rendered = { dataset: { labelIndex: '0' }, getBBox: () => ({ x: 20, y: 30, width: 80, height: 24 }) };
  editor.svgLayerEl.querySelectorAll = () => [rendered];
  const attributes = {};
  editor.handlesLayerEl = { querySelector: () => ({ setAttribute(key, value) { attributes[key] = value; } }) };
  editor.scene.annotations = [{ type: 'label', target: [200, 200, 200, 80], position: 'top', text: 'Label' }];
  assert.equal(editor.findAnnotationAt(40, 40), 0);
  assert.equal(editor.findAnnotationAt(250, 230), null);
  assert.deepEqual(JSON.parse(JSON.stringify(editor.getAnnotationBounds())), { minX: 18, minY: 28, maxX: 102, maxY: 56 });
  editor.selectedIndex = 0;
  editor.initialAnnotationState = editor.annotationDragState(0);
  editor.isDraggingAnnotation = true;
  editor.dragStartX = 40; editor.dragStartY = 40;
  editor.handleMouseMove({ x: 250, y: 230, altKey: false });
  assert.deepEqual(Array.from(editor.scene.annotations[0].at), [230, 220]);
  assert.deepEqual(Array.from(editor.scene.annotations[0].target), [200, 200, 200, 80]);
  assert.equal(attributes.x, '230'); assert.equal(attributes.y, '220');
  editor.handleMouseMove({ x: 260, y: 240, altKey: false });
  assert.deepEqual(Array.from(editor.scene.annotations[0].at), [240, 230]);
  editor.isDraggingAnnotation = false;
  const copy = JSON.parse(editor.copySelectedMarks()).annotations[0];
  assert.deepEqual(copy.at, [240, 230]);
  editor.svgLayerEl.querySelectorAll = () => [];
  assert.equal(editor.findAnnotationAt(250, 230), null);
  editor.activeTool = 'label';
  assert.equal(editor.creationSnap(250, 230, false).snapped, false);
  assert.deepEqual(Array.from(editor.createDefaultAnnotation('label', 60, 70).at), [60, 70]);
});

test('unified rectangle corner toggle controls rendering and saving without losing line, fill or radius settings', () => {
  const { editor } = fixture();
  const rectangle = editor.createDefaultAnnotation('rect', 100, 100);
  Object.assign(rectangle, { line_style: 'dotted', fill_style: 'none', stroke_width: 5, rx: 15, ry: 10 });
  editor.scene.annotations = [rectangle];
  assert.equal(editor.getCleanScene().annotations[0].type, 'rect');
  rectangle.rounded = true;
  let clean = editor.getCleanScene().annotations[0];
  assert.equal(clean.type, 'rounded-rect');
  assert.equal(clean.rx, 15); assert.equal(clean.ry, 10);
  assert.equal(clean.line_style, 'dotted'); assert.equal(clean.fill_style, 'none');
  rectangle.rounded = false;
  clean = editor.getCleanScene().annotations[0];
  assert.equal(clean.type, 'rect'); assert.equal(clean.rx, undefined);
  rectangle.rounded = true;
  assert.equal(editor.getCleanScene().annotations[0].rx, 15);
});

test('rectangle roundness ranges from square to fully rounded for small and large shapes', () => {
  const { editor } = fixture();
  editor.scene.annotations = [
    { type: 'rect', target: [10, 20, 100, 50], line_style: 'dashed', fill_style: 'none' },
    { type: 'rect', target: [200, 100, 800, 400], line_style: 'dotted' },
  ];
  for (const percent of [0, 1, 50, 100]) {
    for (let index = 0; index < 2; index++) {
      editor.setRectangleRoundness(index, percent);
      const annotation = editor.scene.annotations[index];
      assert.equal(annotation.rx, (index === 0 ? 25 : 200) * percent / 100);
      assert.equal(annotation.ry, annotation.rx);
      assert.equal(editor.rectangleRoundness(annotation), percent);
      assert.equal(editor.getCleanScene().annotations[index].type, percent ? 'rounded-rect' : 'rect');
    }
  }
  editor.setRectangleRoundness(0, 0);
  assert.equal(editor.getCleanScene().annotations[0].type, 'rect');
  assert.equal(editor.scene.annotations[0].line_style, 'dashed');
  assert.equal(editor.scene.annotations[0].fill_style, 'none');
});

test('Alt, leaving the canvas, leaving candidates, and select mode remove preview', () => {
  const { editor, hover } = fixture();
  hover(); hover(250, 230, true); assert.equal(editor.hoverPreview, null);
  hover(); hover(-1, -1); assert.equal(editor.hoverPreview, null);
  hover(); hover(800, 700); assert.equal(editor.hoverPreview, null);
  hover(); editor.activeTool = 'select'; hover(); assert.equal(editor.hoverPreview, null);
});

test('identical hovers reuse the preview and stale async renders cannot restore it', async () => {
  const { editor, hover } = fixture();
  hover();
  const revision = editor.hoverPreviewRevision;
  hover();
  assert.equal(editor.hoverPreviewRevision, revision);
  let finish;
  editor.renderCallback = () => new Promise(resolve => { finish = resolve; });
  const render = editor.executeRender();
  editor.clearSnapGuide();
  finish('<svg>stale preview</svg>');
  await render;
  assert.equal(editor.svgLayerEl.innerHTML, '');
  assert.equal(editor.hoverPreview, null);
});

test('mosaic previews and creation preserve large target bounds', async () => {
  // Exercise absolute size, relative size, and canvas coverage limits.
  for (const [width, height] of [[200, 80], [651, 300], [600, 451], [900, 650], [1800, 950]]) {
    const { editor, hover } = fixture();
    editor.scene.canvas = { width: 1920, height: 1080 };
    editor.activeTool = 'mosaic';
    editor.uiElements = [{ x: 50, y: 50, width, height, name: 'Panel', role: 'pane' }];
    const expected = [50, 50, width, height];
    for (const [x, y] of [[100, 90], [50 + width / 2, 50 + height / 2]]) {
      hover(x, y);
      assert.deepEqual(Array.from(editor.hoverPreview.target), expected);
      const created = editor.createSnappedAnnotation('mosaic', editor.creationSnap(x, y, false));
      assert.deepEqual(Array.from(created.target), expected);
    }
    let rendered;
    editor.renderCallback = async json => { rendered = JSON.parse(json); return '<svg/>'; };
    await editor.executeRender();
    assert.deepEqual(rendered.annotations[0].target, expected);
    assert.equal(editor.scene.annotations.length, 0);
    hover(100, 90, true);
    assert.equal(editor.hoverPreview, null);
  }
});

test('large mosaic targets remain available while dragging and moving', () => {
  const { editor } = fixture();
  editor.scene.canvas = { width: 1920, height: 1080 };
  editor.activeTool = 'mosaic';
  editor.uiElements = [{ x: 50, y: 50, width: 900, height: 650, name: 'Panel', role: 'pane' }];
  assert.deepEqual(Array.from(editor.findSnapElementForRect(52, 52, 948, 698).rect), [50, 50, 900, 650]);
  assert.deepEqual(Array.from(editor.findSnapElementForMove(52, 52, 900, 650).rect), [50, 50, 900, 650]);
  editor.activeTool = 'rect';
  assert.equal(editor.findSnapElementForRect(52, 52, 948, 698).snapped, false);
  assert.equal(editor.findSnapElementForMove(52, 52, 900, 650).snapped, false);
});

test('mosaic still prefers a smaller UI element inside a large panel', () => {
  const { editor, hover } = fixture();
  editor.activeTool = 'mosaic';
  editor.uiElements.push({ x: 0, y: 0, width: 1000, height: 800, name: 'Panel', role: 'pane' });
  hover();
  assert.deepEqual(Array.from(editor.hoverPreview.target), [200, 200, 200, 80]);
});
