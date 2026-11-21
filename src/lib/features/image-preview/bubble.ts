// The preview's frame: a rounded rectangle and a callout arrow on the panel's side, one closed
// path so no seam shows where they meet. Sizes match `preview.rs`, which sizes the window to the
// image plus this frame.

/** Corner radius, arrow depth and half its height, in CSS pixels. */
export const RADIUS = 8;
export const ARROW_WIDTH = 8;
export const ARROW_HALF = 8;

/**
 * The outline of a `width` × `height` frame with its arrow on the right at `arrowY`, inset by
 * `inset` so a stroke twice as wide stays inside the viewport. A frame for the left side is this
 * one mirrored.
 */
export function bubblePath(width: number, height: number, arrowY: number, inset: number): string {
  const [left, top, bottom] = [inset, inset, height - inset];
  const right = width - ARROW_WIDTH;
  const tip = width - inset;
  const r = RADIUS;
  return [
    `M ${left + r} ${top}`,
    `L ${right - r} ${top}`,
    `A ${r} ${r} 0 0 1 ${right} ${top + r}`,
    `L ${right} ${arrowY - ARROW_HALF}`,
    `L ${tip} ${arrowY}`,
    `L ${right} ${arrowY + ARROW_HALF}`,
    `L ${right} ${bottom - r}`,
    `A ${r} ${r} 0 0 1 ${right - r} ${bottom}`,
    `L ${left + r} ${bottom}`,
    `A ${r} ${r} 0 0 1 ${left} ${bottom - r}`,
    `L ${left} ${top + r}`,
    `A ${r} ${r} 0 0 1 ${left + r} ${top}`,
    "Z",
  ].join(" ");
}
