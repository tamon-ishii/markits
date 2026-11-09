import type { Annotation } from './types.ts';

export function importAnnotations(raw: unknown, hidden: unknown = []) {
  const annotations: Annotation[] = [];
  const hidden_annotations: number[] = [];
  const hiddenSet = new Set(Array.isArray(hidden) ? hidden : []);
  if (!Array.isArray(raw)) return { annotations, hidden_annotations };
  const aliases: Record<string, string> = { rounded_rect: 'rounded-rect', ellipse: 'circle', step_arrow: 'step-arrow' };
  const supported = new Set(['arrow', 'bezier-arrow', 'step-arrow', 'badge', 'label', 'callout', 'rect', 'rounded-rect', 'circle', 'mosaic', 'spotlight']);
  raw.forEach((value, index) => {
    if (!value || typeof value !== 'object' || typeof value.type !== 'string') return;
    const type = aliases[value.type] ?? value.type;
    if (!supported.has(type)) return;
    if (hiddenSet.has(index)) hidden_annotations.push(annotations.length);
    annotations.push(type === 'step-arrow' ? { ...value, type: 'badge', arrow: true, target_box: false }
      : type === 'rounded-rect' ? { ...value, type: 'rect', rounded: true, rx: value.rx ?? 8, ry: value.ry ?? 8 }
      : { ...value, type });
  });
  return { annotations, hidden_annotations };
}
