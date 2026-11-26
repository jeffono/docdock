import type { TabWheelMode } from "./ipc";

export const panelTabs = ["notes", "scratchpad", "browser"] as const;
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

/**
 * Accumulates wheel deltas until one switch threshold is crossed, preventing smooth touchpads
 * from skipping multiple tabs on one gesture while firing once per discrete mouse wheel detent.
 */
export function createWheelStepper(threshold = 50, idleMs = 200) {
  let accumulated = 0;
  let last = 0;
  return {
    step(delta: number, now = Date.now()): 1 | -1 | null {
      if (delta === 0) return null;
      if (
        now - last > idleMs ||
        (accumulated !== 0 && Math.sign(delta) !== Math.sign(accumulated))
      ) {
        accumulated = 0;
      }
      last = now;
      accumulated += delta;
      if (Math.abs(accumulated) < threshold) return null;
      const direction = accumulated > 0 ? 1 : -1;
      accumulated = 0;
      return direction;
    },
    reset() {
      accumulated = 0;
      last = 0;
    },
  };
}
