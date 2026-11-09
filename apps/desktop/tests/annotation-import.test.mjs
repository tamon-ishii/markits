import { test } from 'node:test';
import assert from 'node:assert/strict';
import { importAnnotations } from '../src/annotation-import.ts';
import { decodeAnnotations, encodeAnnotations } from '../src/annotation-clipboard.ts';

test('removed and unsupported marks are silently skipped and hidden indices are remapped', () => {
  const input = [
    { type: 'pin', target: 'missing' },
    { type: 'rect', target: [1, 2, 30, 40] },
    { type: 'bullseye' }, { type: 'future-mark' }, { type: 'divider' },
    { type: 'label', text: 'Label', at: [50, 60], target: [0, 0, 1, 1] },
    { type: 'step-arrow', target: [80, 90, 20, 20], step: 2 }, null,
  ];
  const result = importAnnotations(input, [0, 5, 6]);
  assert.deepEqual(result.annotations.map(a => a.type), ['rect', 'label', 'badge']);
  assert.deepEqual(result.hidden_annotations, [1, 2]);
  assert.equal(result.annotations[2].arrow, true);
  assert.deepEqual(decodeAnnotations(encodeAnnotations(input)).map(a => a.type), ['rect', 'label', 'badge']);
  assert.deepEqual(importAnnotations([{ type: 'missing' }]).annotations, []);
});

test('old rounded rectangles become rectangles with rounded corners and preserve their appearance', () => {
  const result = importAnnotations([
    { type: 'rounded-rect', target: [10, 20, 100, 50], rx: 15, ry: 9, line_style: 'dashed', fill_style: 'none' },
    { type: 'rounded_rect', target: [20, 30, 40, 50] },
    { type: 'rect', target: [30, 40, 50, 60] },
  ]);
  assert.deepEqual(result.annotations.map(a => a.type), ['rect', 'rect', 'rect']);
  assert.equal(result.annotations[0].rounded, true);
  assert.equal(result.annotations[0].rx, 15);
  assert.equal(result.annotations[0].ry, 9);
  assert.equal(result.annotations[0].line_style, 'dashed');
  assert.equal(result.annotations[0].fill_style, 'none');
  assert.equal(result.annotations[1].rx, 8);
  assert.equal(result.annotations[2].rounded, undefined);
  assert.deepEqual(decodeAnnotations(encodeAnnotations(result.annotations)), result.annotations);
});
