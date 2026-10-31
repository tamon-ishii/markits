const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');

function setup(language = 'en-US', saved) {
  const values = new Map(saved ? [['markits.locale', saved]] : []);
  const exports = {};
  const source = fs.readFileSync(path.join(__dirname, '../src/i18n.ts'), 'utf8');
  const code = ts.transpileModule(source, { compilerOptions: {
    module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true,
  } }).outputText;
  vm.runInNewContext(code, {
    exports, require: () => require('../src/locales/en.json'),
    navigator: { language },
    localStorage: { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) },
  });
  return exports;
}

test('system language, saved preference, invalid preference and persistence', () => {
  assert.equal(setup('ja-JP').getLocale(), 'ja');
  assert.equal(setup('de-DE').getLocale(), 'en');
  const i18n = setup('en-US', 'ja');
  assert.equal(i18n.getLocale(), 'ja');
  i18n.saveLocale('en');
  assert.equal(i18n.getLocale(), 'en');
  assert.equal(setup('ja-JP', 'invalid').getLocale(), 'ja');
  assert.throws(() => i18n.saveLocale('fr'));
});

test('Japanese source text remains unchanged', () => {
  const { t } = setup('ja-JP');
  assert.equal(t('操作を記録'), '操作を記録');
  assert.equal(t`新しく撮影した画像：${3} 枚`, '新しく撮影した画像：3 枚');
});

test('English UI text, tooltips and plural-neutral counters', () => {
  const { t } = setup();
  assert.equal(t('⏺ 操作を記録'), '⏺ Record actions');
  assert.equal(t('画像をクリップボードにコピー'), 'Copy image to clipboard');
  assert.equal(t`新しく撮影した画像：${3} 枚`, 'New captures: 3');
  assert.equal(t`操作 ${2} の前の待機時間（ミリ秒）`, 'Delay before operation 2 (ms)');
});

test('interpolated user content is never translated or interpreted as placeholders', () => {
  const { t } = setup();
  const name = '操作を記録 {1} 保存';
  assert.equal(t`対象：${name}`, `Target: ${name}`);
  assert.equal(t`【名称で照合】「${name}」／種類：${'button'}／記録位置：${'(1, 2)'}`,
    `[Match by name] ${name} / Role: button / Recorded position: (1, 2)`);
});

test('unknown text falls back to its source', () => {
  assert.equal(setup().t('not in the catalog'), 'not in the catalog');
});
