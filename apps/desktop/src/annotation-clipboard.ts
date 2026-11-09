import type { Annotation } from './types.ts';
import { importAnnotations } from './annotation-import.ts';

export function encodeAnnotations(annotations: Annotation[]): string {
  return JSON.stringify({ format: 'markits-annotations', version: 1, annotations });
}

export function decodeAnnotations(text: string): Annotation[] | null {
  try {
    const data = JSON.parse(text);
    if (data.format !== 'markits-annotations' || data.version !== 1 ||
        !Array.isArray(data.annotations) || !data.annotations.length || data.annotations.length > 1000) return null;
    const point = (value: unknown, length: number) => Array.isArray(value) && value.length === length && value.every(Number.isFinite);
    const annotations = importAnnotations(data.annotations).annotations;
    if (!annotations.length) return null;
    for (const a of annotations) {
      if (!a || typeof a !== 'object') return null;
      if (a.type === 'arrow' || a.type === 'bezier-arrow') {
        if (!point(a.start, 2) || !point(a.end, 2) || (a.type === 'bezier-arrow' && !point(a.control, 2))) return null;
      } else if (['badge', 'step-arrow', 'callout', 'label', 'pin', 'rect', 'rounded-rect', 'circle', 'mosaic', 'spotlight', 'bullseye', 'divider'].includes(a.type)) {
        if (!point(a.target, 4) || a.target[2] <= 0 || a.target[3] <= 0) return null;
      } else return null;
      if ((a.type === 'callout' || a.type === 'label') && typeof a.text !== 'string') return null;
      if (a.type === 'label' && a.at !== undefined && !point(a.at, 2)) return null;
    }
    return annotations;
  } catch { return null; }
}

export function offsetAnnotations(annotations: Annotation[], offset: number): Annotation[] {
  const copies = JSON.parse(JSON.stringify(annotations));
  for (const a of copies) {
    delete a.id;
    if (a.target) { a.target[0] += offset; a.target[1] += offset; }
    for (const key of ['start', 'end', 'control', 'at']) {
      if (a[key]) { a[key][0] += offset; a[key][1] += offset; }
    }
  }
  return copies;
}
