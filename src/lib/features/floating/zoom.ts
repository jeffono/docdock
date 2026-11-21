// Zooming a floating window's image with Ctrl+wheel and panning it with Ctrl+drag: pure rules
// whose effects `FloatWindow.svelte` runs. The window grows first, up to the side preview's share
// of the monitor (Rust decides it); past that the image zooms inside it.

/** One wheel notch. */
export const ZOOM_STEP = 1.15;
/** The most the image zooms inside its window. */
export const MAX_ZOOM = 8;
/** Within this many CSS pixels of the center, a pan settles on it. */
export const PAN_SNAP = 6;

export type Size = { width: number; height: number };
export type Point = { x: number; y: number };

/** Resize the window so the image is `width` wide, or zoom inside it to `zoom`. */
export type ZoomStep = { kind: "resize"; width: number } | { kind: "inner"; zoom: number };

/**
 * The next step in or out for the image's `box` (its CSS size now), `natural` size (zero while
 * unknown) and zoom inside the window, when the window is already as large as it gets (`atMax`).
 * Growing or shrinking through the natural size stops there first (1:1); zooming out of the
 * window stops at 1 before the window shrinks.
 */
export function zoomStep(
  box: Size,
  natural: Size,
  zoom: number,
  atMax: boolean,
  zoomIn: boolean,
): ZoomStep {
  if (zoomIn) {
    if (zoom > 1 || atMax) return { kind: "inner", zoom: Math.min(MAX_ZOOM, zoom * ZOOM_STEP) };
    const width = box.width * ZOOM_STEP;
    const through = box.width < natural.width - 0.5 && width > natural.width;
    return { kind: "resize", width: through ? natural.width : width };
  }
  if (zoom > 1) {
    const next = zoom / ZOOM_STEP;
    return { kind: "inner", zoom: next < 1.001 ? 1 : next };
  }
  const width = box.width / ZOOM_STEP;
  const through = box.width > natural.width + 0.5 && width < natural.width;
  return { kind: "resize", width: through ? natural.width : width };
}

/**
 * `pan` (an offset from the center) kept so an image zoomed by `zoom` in `stage` never pulls away
 * from its edges, settling on the center when within `PAN_SNAP`.
 */
export function clampPan(pan: Point, zoom: number, stage: Size): Point {
  const reach = (length: number) => (Math.max(zoom, 1) - 1) * (length / 2);
  const limit = (value: number, length: number) =>
    Math.min(reach(length), Math.max(-reach(length), value));
  const x = limit(pan.x, stage.width);
  const y = limit(pan.y, stage.height);
  return Math.hypot(x, y) < PAN_SNAP ? { x: 0, y: 0 } : { x, y };
}

/**
 * The pan that keeps the image's point under `pointer` (from the stage's center) in place when the
 * zoom goes from `zoom` to `next`, within the stage.
 */
export function zoomAt(pan: Point, zoom: number, next: number, pointer: Point, stage: Size): Point {
  const ratio = next / zoom;
  const moved = {
    x: pointer.x - (pointer.x - pan.x) * ratio,
    y: pointer.y - (pointer.y - pan.y) * ratio,
  };
  return clampPan(moved, next, stage);
}
