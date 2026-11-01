const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');

function fixture({ zoom = 0.7, outputX = 1, outputY = 1, auto = false, offsets = [{ x: 300, y: 180 }] } = {}) {
  const modules = new Map();
  function load(file) {
    if (file.endsWith('.json')) return { default: JSON.parse(fs.readFileSync(file, 'utf8')) };
    if (modules.has(file)) return modules.get(file);
    const exports = {};
    modules.set(file, exports);
    const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), { compilerOptions: {
      module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022,
    } }).outputText;
    vm.runInNewContext(code, { exports, console, navigator: { language: 'ja' },
      document: { getElementById: () => null },
      require: id => load(path.resolve(path.dirname(file), id)),
    });
    return exports;
  }
  const { AnnotationEditor } = load(path.join(__dirname, '../src/editor.ts'));
  const editor = Object.create(AnnotationEditor.prototype);
  const total = offsets.reduce((sum, offset) => ({ x: sum.x + offset.x, y: sum.y + offset.y }), { x: 0, y: 0 });
  Object.assign(editor, {
    scene: { canvas: { width: 400, height: 240 }, annotations: [] },
    outputWidth: 400 * outputX, outputHeight: 240 * outputY, zoomLevel: zoom,
    baseImageState: { dataUrl: 'base', width: 1600, height: 1000, uiElements: [] },
    isAutoCropped: auto, autoCropOffset: total,
    cropHistoryStack: auto ? [] : offsets.map(offset => ({ offset })),
    canvasContainerEl: { style: { translate: '', setProperty() {} } },
    editorCropOverlayEl: { style: {} }, snapGuideLayerEl: { innerHTML: '' },
    hasImage: () => true, setHoverPreview() {}, updateHandles() {}, updateInspector() {},
    updateCropOverlayUi() {}, renderAllSnapTargets() {},
    // Isolate view positioning from pixel decoding and history persistence.
    revertCrop() {
      this.scene.canvas = { width: 1600, height: 1000 };
      this.outputWidth = 1600; this.outputHeight = 1000;
      this.isAutoCropped = false; this.cropHistoryStack = [];
      this.autoCropOffset = { x: 0, y: 0 };
      this.setZoom(this.zoomLevel);
    },
  });
  editor.canvasContainerEl.getBoundingClientRect = () => {
    const { width, height } = editor.scene.canvas;
    const [sx, sy] = editor.canvasContainerEl.style.transform.match(/[\d.]+/g).map(Number);
    const [tx = 0, ty = 0] = editor.canvasContainerEl.style.translate.split(' ').map(parseFloat);
    return {
      left: Math.max(24, (1000 - width) / 2) + width * (1 - sx) / 2 + (tx || 0),
      top: Math.max(24, (700 - height) / 2) + height * (1 - sy) / 2 + (ty || 0),
      width: width * sx, height: height * sy,
    };
  };
  editor.setZoom(zoom);
  return { editor, total };
}

test('reopening manual, nested, and automatic crops keeps screen position and size', () => {
  for (const options of [
    {}, { zoom: 1.4 }, { zoom: 0.4, outputX: 1.5, outputY: 0.75 },
    { auto: true }, { offsets: [{ x: 200, y: 100 }, { x: 90, y: 60 }] },
  ]) {
    const { editor, total } = fixture(options);
    const before = editor.canvasContainerEl.getBoundingClientRect();
    editor.startCropMode();
    const frame = editor.getCropRect();
    assert.deepEqual(JSON.parse(JSON.stringify(frame)), { ...total, width: 400, height: 240 });
    const after = editor.canvasContainerEl.getBoundingClientRect();
    const sx = after.width / 1600, sy = after.height / 1000;
    for (const [actual, expected] of [
      [after.left + frame.x * sx, before.left], [after.top + frame.y * sy, before.top],
      [frame.width * sx, before.width], [frame.height * sy, before.height],
    ]) assert.ok(Math.abs(actual - expected) < 0.001, `${actual} != ${expected}`);
    editor.cancelCropMode();
    assert.equal(editor.canvasContainerEl.style.translate, '');
    assert.equal(editor.cropViewScale, null);
  }
});

test('an uncropped image opens a full-image frame without adding a translation', () => {
  const { editor } = fixture();
  editor.cropHistoryStack = [];
  editor.startCropMode();
  assert.deepEqual(JSON.parse(JSON.stringify(editor.getCropRect())), { x: 0, y: 0, width: 1600, height: 1000 });
  assert.equal(editor.canvasContainerEl.style.translate, '');
});
