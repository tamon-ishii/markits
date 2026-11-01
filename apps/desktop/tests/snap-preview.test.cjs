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
  for (const tool of ['arrow', 'bezier-arrow', 'badge', 'step-arrow', 'label', 'rect', 'rounded-rect', 'circle', 'mosaic', 'spotlight', 'bullseye', 'divider', 'callout', 'pin']) {
    const { editor, hover } = fixture();
    editor.activeTool = tool;
    hover();
    let rendered;
    editor.renderCallback = async json => { rendered = JSON.parse(json); return '<svg>preview</svg>'; };
    await editor.executeRender();
    assert.equal(rendered.annotations.length, 1, tool);
    assert.equal(rendered.annotations[0].type, tool);
    assert.equal(editor.scene.annotations.length, 0);
    assert.equal(editor.getCleanScene().annotations.length, 0);
  }
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
