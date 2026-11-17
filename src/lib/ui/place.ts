/**
 * The panel's columns in the viewport, which reaches beyond the panel by the edge tab's strip, as
 * `--frame-left` and `--frame-right` on the panel say.
 */
export function panelColumns(inside: Element) {
  const style = getComputedStyle(inside);
  const frame = (name: string) => Number.parseFloat(style.getPropertyValue(name)) || 0;
  return { left: frame("--frame-left"), right: innerWidth - frame("--frame-right") };
}

/**
 * Places an open modal dialog just below `near`, what it is about, or above it when there is no room,
 * always inside the viewport; centered without `near`. The dialog's top margin keeps the gap.
 */
export function placeNear(dialog: HTMLElement, near?: DOMRect) {
  const height = dialog.getBoundingClientRect().height;
  const gap = Number.parseFloat(getComputedStyle(dialog).marginTop) || 0;
  const top = !near
    ? (innerHeight - height) / 2
    : near.bottom + gap + height <= innerHeight - gap
      ? near.bottom
      : near.top - height - 2 * gap;
  dialog.style.top = `${Math.max(0, Math.min(top, innerHeight - height - 2 * gap))}px`;
}
