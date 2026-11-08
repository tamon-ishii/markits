const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');

// Exercise the real selection handlers without injecting input into desktop apps.
class Element {
  constructor(classes = '') {
    this.classes = new Set(classes.split(/\s+/).filter(Boolean));
    this.classList = {
      add: name => this.classes.add(name), remove: name => this.classes.delete(name),
      contains: name => this.classes.has(name),
      toggle: (name, active) => active ? this.classes.add(name) : this.classes.delete(name),
    };
    this.listeners = new Map(); this.style = {}; this.checked = false; this.disabled = false;
  }
  addEventListener(name, callback) {
    this.listeners.set(name, [...(this.listeners.get(name) ?? []), callback]);
  }
  emit(name) { for (const callback of this.listeners.get(name) ?? []) callback({ target: this }); }
  setAttribute() {}
  closest() { return null; }
}

function fixture() {
  const html = fs.readFileSync(path.join(__dirname, '../index.html'), 'utf8');
  const elements = new Map([...html.matchAll(/<[^>]+\bid="([^"]+)"[^>]*>/g)].map(match =>
    [match[1], new Element(match[0].match(/class="([^"]*)"/)?.[1])]));
  const hint = new Element();
  const document = { getElementById: id => elements.get(id) ?? null, querySelector: () => hint };
  const window = { innerWidth: 1200, innerHeight: 800, addEventListener() {} };
  const modules = new Map();
  function load(file) {
    if (file.endsWith('.json')) return { default: JSON.parse(fs.readFileSync(file, 'utf8')) };
    if (modules.has(file)) return modules.get(file);
    const module = { exports: {} }; modules.set(file, module.exports);
    const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
    }).outputText;
    vm.runInNewContext(code, { module, exports: module.exports, document, window, navigator: { language: 'ja-JP' },
      require: id => load(path.resolve(path.dirname(file), id)), alert: message => { throw new Error(message); } });
    return module.exports;
  }
  const { CaptureOverlay } = load(path.join(__dirname, '../src/overlay.ts'));
  const overlay = new CaptureOverlay();
  Object.assign(elements.get('overlay-bg'), { complete: true, naturalWidth: 1200, naturalHeight: 800 });
  const target = { role: 'window', name: 'Target', window_id: 'hwnd:0x123', pid: 42, x: 50, y: 50, width: 1000, height: 700 };
  const shots = [], pending = [], recordings = [];
  let cancelled = 0;
  overlay.setCursorSupported(true);
  overlay.show('data:image/png;base64,test', [target], rect => shots.push(rect), () => cancelled++,
    (win, includeCursor) => recordings.push({ win, includeCursor }), request => pending.push(request), false, 'PrintScreen');
  const event = (x, y) => ({ button: 0, clientX: x, clientY: y, target: elements.get('overlay-bg') });
  const clickWindow = () => overlay.handleWindowClick(event(150, 150));
  const check = checked => { const input = elements.get('capture-include-cursor'); input.checked = checked; input.emit('change'); };
  return { overlay, elements, target, shots, pending, recordings, check, event, clickWindow, cancelled: () => cancelled };
}

test('region mode captures a reverse drag without choosing a window at HiDPI scale', () => {
  const f = fixture();
  Object.assign(f.elements.get('overlay-bg'), { naturalWidth: 2400, naturalHeight: 1600 });
  f.elements.get('btn-mode-region').emit('click');
  f.overlay.startRegion({ ...f.event(400, 300), preventDefault() {} });
  f.overlay.handleMouseMove(f.event(100, 80));
  f.overlay.finishRegion(f.event(100, 80));
  assert.deepEqual(JSON.parse(JSON.stringify(f.shots)), [{ x: 200, y: 160, width: 600, height: 440 }]);
  f.clickWindow();
  assert.equal(f.shots.length, 1);
});

test('region mode ignores clicks and toolbar drags and Escape cancels a drag', () => {
  const f = fixture();
  f.elements.get('btn-mode-region').emit('click');
  f.overlay.startRegion({ ...f.event(20, 20), preventDefault() {} });
  f.overlay.finishRegion(f.event(20, 20));
  assert.equal(f.shots.length, 0);
  f.overlay.startRegion({ ...f.event(20, 20), target: { closest: () => ({}) }, preventDefault() { throw Error('toolbar drag'); } });
  f.overlay.finishRegion(f.event(60, 60));
  assert.equal(f.shots.length, 0);
  f.overlay.startRegion({ ...f.event(20, 20), preventDefault() {} });
  f.overlay.cancel();
  f.overlay.finishRegion(f.event(60, 60));
  assert.equal(f.shots.length, 0);
  assert.equal(f.cancelled(), 1);
});

test('region with cursor uses the selected bounds and can switch back to window mode', () => {
  const f = fixture();
  f.elements.get('btn-mode-region').emit('click');
  f.check(true);
  f.overlay.startRegion({ ...f.event(10, 20), preventDefault() {} });
  f.overlay.finishRegion(f.event(80, 100));
  assert.equal(f.pending[0].kind, 'region');
  assert.deepEqual(JSON.parse(JSON.stringify(f.pending[0].rect)), { x: 10, y: 20, width: 70, height: 80 });
  assert.equal(f.shots.length, 0);
  const g = fixture();
  g.elements.get('btn-mode-region').emit('click');
  g.elements.get('btn-mode-capture').emit('click');
  g.clickWindow();
  assert.equal(g.shots.length, 1);
});

test('checking the cursor option does not select even a highlighted window or enter capture waiting', () => {
  const f = fixture();
  f.overlay.handleMouseMove(f.event(150, 150));
  f.check(true);
  assert.equal(f.pending.length, 0);
  assert.equal(f.shots.length, 0);
  assert.equal(f.elements.get('capture-overlay').classList.contains('hidden'), false);
  assert.equal(f.elements.get('btn-capture-fullscreen').disabled, false);
  f.clickWindow();
  assert.equal(f.pending.length, 1);
  assert.equal(f.pending[0].kind, 'window');
  assert.equal(f.pending[0].window.window_id, f.target.window_id);
  assert.equal(f.elements.get('capture-overlay').classList.contains('hidden'), true);
});

test('unchecked window capture stays immediate and unchecking never starts capture', () => {
  const f = fixture();
  f.check(true); f.check(false);
  assert.equal(f.pending.length + f.shots.length, 0);
  f.clickWindow();
  assert.equal(f.shots.length, 1);
  assert.equal(f.pending.length, 0);
});

test('moving the pointer does not create a region or enter capture waiting', () => {
  const f = fixture(); f.check(true);
  f.overlay.handleMouseMove(f.event(100, 100));
  f.overlay.handleMouseMove(f.event(300, 240));
  assert.equal(f.pending.length, 0);
  assert.equal(f.shots.length, 0);
  f.overlay.startRegion({ ...f.event(100, 100), preventDefault() { throw Error('window mode must not drag'); } });
  f.overlay.finishRegion(f.event(300, 240));
  assert.equal(f.pending.length, 0);
  assert.equal(f.shots.length, 0);
  assert.equal(f.elements.get('capture-overlay').listeners.has('mouseup'), false);
  f.clickWindow();
  assert.equal(f.pending[0].kind, 'window');
});

test('window selection uses the clicked window instead of the previously highlighted one', () => {
  const f = fixture();
  f.overlay.handleMouseMove(f.event(150, 150));
  f.check(true);
  const other = { ...f.target, window_id: 'hwnd:0x456', x: 1060, width: 100 };
  f.overlay.uiElements.push(other);
  f.overlay.handleWindowClick(f.event(1100, 150));
  assert.equal(f.pending[0].window.window_id, other.window_id);
});

test('fullscreen uses keyboard waiting only when cursor inclusion is checked', () => {
  for (const included of [false, true]) {
    const f = fixture(); f.check(included);
    f.elements.get('btn-capture-fullscreen').emit('click');
    assert.equal(f.shots.length, included ? 0 : 1);
    assert.equal(f.pending.length, included ? 1 : 0);
    if (included) assert.equal(f.pending[0].kind, 'fullscreen');
    else assert.equal(f.shots[0], null);
  }
});

test('recording mode keeps the cursor option and starts recording instead of a standalone shot', () => {
  const f = fixture(); f.check(true);
  f.elements.get('btn-mode-record').emit('click');
  assert.equal(f.elements.get('capture-include-cursor').checked, true);
  assert.equal(f.elements.get('btn-capture-fullscreen').disabled, true);
  assert.equal(f.recordings.length, 0);
  f.clickWindow();
  assert.equal(f.recordings.length, 1);
  assert.equal(f.recordings[0].includeCursor, true);
  assert.equal(f.pending.length + f.shots.length, 0);
});

test('cancel before selecting a target does not arm cursor capture', () => {
  const f = fixture(); f.check(true);
  f.elements.get('btn-cancel-capture').emit('click');
  assert.equal(f.cancelled(), 1);
  assert.equal(f.pending.length + f.recordings.length + f.shots.length, 0);
});
