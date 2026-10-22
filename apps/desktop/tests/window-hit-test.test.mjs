import { test } from 'node:test';
import assert from 'node:assert/strict';
import { findWindowAt } from '../src/window-hit-test.ts';

const background = { role: 'window', name: 'PyCharm', x: 0, y: 0, width: 1000, height: 800 };
const foreground = { role: 'window', name: 'Browser', x: 100, y: 100, width: 400, height: 300 };

test('overlap selects the foreground window, exposed area selects PyCharm', () => {
  assert.equal(findWindowAt([background, foreground], 200, 200), foreground);
  assert.equal(findWindowAt([background, foreground], 600, 200), background);
});
test('a fully covered window cannot be selected', () => {
  const cover = { ...background, name: 'Cover' };
  assert.equal(findWindowAt([background, cover], 500, 400), cover);
});
test('small foreground windows block selection through to the background', () => {
  assert.equal(findWindowAt([background, { ...foreground, width: 20 }], 110, 200), null);
});
test('containers are not a fallback and right/bottom boundaries are exclusive', () => {
  assert.equal(findWindowAt([{ ...background, role: 'pane' }], 200, 200), null);
  assert.equal(findWindowAt([background, foreground], 500, 200), background);
  assert.equal(findWindowAt([background], 1000, 800), null);
});
