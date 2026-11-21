// Opacity steps of a floating window, from the slider or the mouse wheel. Rust clamps again.

export const MIN_OPACITY = 0.15;
/** One wheel notch or slider step. */
export const OPACITY_STEP = 0.05;

/** `current` one step more opaque (`up`) or fainter, on the step grid and within bounds. */
export function stepOpacity(current: number, up: boolean): number {
  const steps = Math.round(current / OPACITY_STEP) + (up ? 1 : -1);
  const next = Math.round(steps * OPACITY_STEP * 100) / 100;
  return Math.min(1, Math.max(MIN_OPACITY, next));
}
