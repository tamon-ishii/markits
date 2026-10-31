import english from './locales/en.json';

export type Locale = 'ja' | 'en';
const storageKey = 'markits.locale';
const catalog: Record<string, string> = english;
const phrases = new RegExp(Object.keys(catalog).sort((a, b) => b.length - a.length)
  .map(key => key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|'), 'g');

export function getLocale(): Locale {
  try {
    const saved = localStorage.getItem(storageKey);
    if (saved === 'ja' || saved === 'en') return saved;
  } catch { /* Use the system language when storage is unavailable. */ }
  return navigator.language.toLowerCase().startsWith('ja') ? 'ja' : 'en';
}

function translate(source: string): string {
  if (getLocale() === 'ja') return source;
  return catalog[source] ?? source.replace(phrases, key => catalog[key]);
}

/** Translate authored text before inserting values, keeping user content untouched. */
export function t(source: string | TemplateStringsArray, ...values: unknown[]): string {
  if (typeof source === 'string') return translate(source);
  const key = source.reduce((result, part, index) => result + (index ? `{${index - 1}}` : '') + part, '');
  const translated = translate(key);
  return translated.replace(/\{(\d+)\}/g, (placeholder, index: string) =>
    Number(index) < values.length ? String(values[Number(index)]) : placeholder);
}

/** Called once on the app-owned HTML, before any image or annotation is loaded. */
export function initializeI18n(): void {
  document.documentElement.lang = getLocale();
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  let node: Node | null;
  while ((node = walker.nextNode())) {
    if (node.parentElement?.closest('script, style, textarea, [contenteditable]')) continue;
    node.textContent = translate(node.textContent ?? '');
  }
  for (const element of document.querySelectorAll('*')) {
    for (const attribute of ['title', 'aria-label', 'placeholder', 'label', 'alt']) {
      const value = element.getAttribute(attribute);
      if (value) element.setAttribute(attribute, translate(value));
    }
  }
  const select = document.getElementById('app-language') as HTMLSelectElement | null;
  if (select) select.value = getLocale();
}

export function saveLocale(locale: string): void {
  if (locale !== 'ja' && locale !== 'en') throw new Error('Unsupported language');
  localStorage.setItem(storageKey, locale);
}
