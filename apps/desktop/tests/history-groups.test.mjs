import { test } from 'node:test';
import assert from 'node:assert/strict';
import { groupHistory } from '../src/history-groups.ts';

test('capture groups keep all thumbnails in capture order and separate replay batches', () => {
  const items = [
    { id: 'capture_20', operation_group: 'replay' },
    { id: 'capture_10', operation_group: 'original' },
    { id: 'capture_9', operation_group: 'original' },
    { id: 'capture_2', operation_group: 'original' },
    { id: 'capture_1' },
  ];
  assert.deepEqual(groupHistory(items).map(group => group.map(item => item.id)), [
    ['capture_20'], ['capture_2', 'capture_9', 'capture_10'], ['capture_1'],
  ]);
  assert.equal(items[1].id, 'capture_10');
});

test('ordinary screenshots remain independent cards', () => {
  assert.equal(groupHistory([{ id: 'a' }, { id: 'b', operation_group: null }]).length, 2);
  assert.deepEqual(groupHistory([]), []);
});
