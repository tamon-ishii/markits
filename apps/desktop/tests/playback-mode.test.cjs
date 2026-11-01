const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');

function fixture() {
  const elements = new Map();
  const timers = [];
  const calls = [];
  const pending = [];
  const element = id => {
    if (!elements.has(id)) elements.set(id, { checked: true, disabled: false, children: [], textContent: '' });
    return elements.get(id);
  };
  const exports = {};
  const source = fs.readFileSync(path.join(__dirname, '../src/playback.ts'), 'utf8');
  const code = ts.transpileModule(source, { compilerOptions: {
    module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022,
  } }).outputText;
  const t = (value, ...args) => typeof value === 'string' ? value : value.reduce((s, part, i) => s + part + (args[i] ?? ''), '');
  vm.runInNewContext(code, {
    exports, require: () => ({ t }),
    document: { body: { innerHTML: '' }, getElementById: element },
    window: { setTimeout: fn => timers.push(fn) },
  });
  exports.showPlaybackControls(async (command, args) => {
    calls.push({ command, args });
    if (command === 'cmd_playback_status') return new Promise(resolve => pending.push(resolve));
    return false;
  });
  const status = shorten_waits => ({
    running: true, paused: true, shorten_waits, include_cursor: false,
    step: 0, total: 0, message: '', operations: [], completed: [], waits_ms: [],
    wait_total_ms: 0, wait_remaining_ms: 0, captures: 0, error: null,
  });
  return { element, timers, calls, pending, status };
}

const flush = () => new Promise(resolve => setImmediate(resolve));

test('a stale poll cannot undo shortening after the setting command completes', async () => {
  const { element, timers, calls, pending, status } = fixture();
  assert.equal(element('playback-resume').disabled, true);
  const mode = element('playback-wait-mode');
  mode.checked = true;
  await mode.onchange();
  // The earlier poll captured recorded mode before the checkbox was changed.
  pending.shift()(status(false));
  await flush();
  assert.equal(mode.checked, true);
  assert.equal(timers.length, 1, 'discarding stale state must not stop polling');
  timers.shift()();
  pending.shift()(status(true));
  await flush();
  assert.equal(mode.checked, true);
  assert.equal(element('playback-resume').disabled, false);
  await element('playback-resume').onclick();
  const command = calls.find(call => call.command === 'cmd_playback_control');
  assert.equal(command.args.action, 'resume');
  assert.equal(command.args.shortenWaits, true);
});

test('resume and single step send the selected wait mode, including recorded mode', async () => {
  const { element, pending, status, calls } = fixture();
  pending.shift()(status(false));
  await flush();
  assert.equal(element('playback-wait-mode').checked, false);
  await element('playback-step').onclick();
  const command = calls.find(call => call.command === 'cmd_playback_control');
  assert.equal(command.args.action, 'step');
  assert.equal(command.args.shortenWaits, false);
});
