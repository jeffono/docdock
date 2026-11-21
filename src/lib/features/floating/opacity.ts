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

/**
 * Sends a window's changes, such as its opacity, one at a time, the latest queued, since answers
 * may otherwise land out of order. While `busy`, what Rust says is older than the control.
 */
export function latestSender(send: (opacity: number) => Promise<unknown>) {
  let pending: number | null = null;
  let sending = false;
  return {
    busy: () => pending !== null || sending,
    async send(value: number) {
      pending = value;
      if (sending) return;
      sending = true;
      while (pending !== null) {
        const next = pending;
        pending = null;
        await send(next).catch(() => {});
      }
      sending = false;
    },
  };
}
