import type { TabWheelMode } from "./ipc";

export const panelTabs = ["notes", "scratchpad", "clipboard", "browser"] as const;
export type PanelTab = (typeof panelTabs)[number];

export function stepTab(current: PanelTab, step: 1 | -1): PanelTab {
  const index = Math.max(panelTabs.indexOf(current), 0);
  return panelTabs[(index + step + panelTabs.length) % panelTabs.length];
}

export type WheelInput = {
  deltaX: number;
  deltaY: number;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
};

/**
 * Signed wheel delta for switching top-bar tabs, or 0 when ignored. Falls back to `deltaX`
 * because Windows turns Shift + vertical wheel into horizontal wheel deltas.
 */
export function wheelTabDelta(mode: TabWheelMode, event: WheelInput): number {
  if (mode === "off") return 0;
  if (event.ctrlKey || event.altKey || event.metaKey) return 0;
  if (mode === "modifier" && !event.shiftKey) return 0;
  return event.deltaY !== 0 ? event.deltaY : event.deltaX;
}

const WHEEL_GESTURE_PAUSE_MS = 250;

/** Accumulates small wheel deltas and requires a fresh gesture at the two entries to Browser. */
export function createWheelStepper(threshold = 50, idleMs = 200) {
  let accumulated = 0;
  let last = -Infinity;
  let stepped = false;
  return {
    step(current: PanelTab, delta: number, now: number): PanelTab | null {
      if (delta === 0) return null;
      const gap = now - last;
      last = now;
      if (gap >= WHEEL_GESTURE_PAUSE_MS) stepped = false;
      const direction = delta > 0 ? 1 : -1;
      const next = stepTab(current, direction);
      if (stepped && next === "browser") {
        accumulated = 0;
        return null;
      }
      if (gap > idleMs || (accumulated !== 0 && direction !== Math.sign(accumulated))) {
        accumulated = 0;
      }
      accumulated += delta;
      if (Math.abs(accumulated) < threshold) return null;
      accumulated = 0;
      stepped = true;
      return next;
    },
    reset() {
      accumulated = 0;
      last = -Infinity;
      stepped = false;
    },
  };
}
