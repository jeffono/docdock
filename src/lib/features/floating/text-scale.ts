// Text size steps of a post-it, from Ctrl+wheel. Rust clamps again.

export const MIN_TEXT_SCALE = 0.5;
export const MAX_TEXT_SCALE = 3;
/** One wheel notch: a tenth of the panel's size. */
const STEP = 0.1;

/** `current` one step larger (`up`) or smaller, on the step grid and within bounds. */
export function stepTextScale(current: number, up: boolean): number {
  const steps = Math.round(current / STEP) + (up ? 1 : -1);
  const next = Math.round(steps * STEP * 100) / 100;
  return Math.min(MAX_TEXT_SCALE, Math.max(MIN_TEXT_SCALE, next));
}
