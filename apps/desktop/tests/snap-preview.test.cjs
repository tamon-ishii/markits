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
