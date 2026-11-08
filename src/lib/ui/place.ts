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
