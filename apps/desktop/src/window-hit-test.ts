import type { DetectedUiElement } from './types.ts';

/** Windows are ordered from back to front. Only the visible window at this point can win. */
export function findWindowAt(
  elements: DetectedUiElement[], x: number, y: number,
): DetectedUiElement | null {
  for (let i = elements.length - 1; i >= 0; i--) {
    const el = elements[i];
    if (el.role !== 'window' || el.width <= 0 || el.height <= 0) continue;
    if (x >= el.x && x < el.x + el.width && y >= el.y && y < el.y + el.height) {
      // A small foreground window still blocks selection of the window behind it.
      return el.width >= 40 && el.height >= 40 ? el : null;
    }
  }
  return null;
}
