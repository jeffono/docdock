/** Stable preview geometry: animation transforms never participate in hit testing. */
export function dragSlot(
  heights: readonly number[],
  source: number,
  current: number,
  y: number,
): number {
  const order = heights.map((_, index) => index);
  order.splice(current, 0, ...order.splice(source, 1));
  let top = 0;
  const centers = order.map((index) => {
    const center = top + heights[index] / 2;
    top += heights[index];
    return center;
  });
  // Crossing a neighbour's center swaps once. Afterward its center is on the other
  // side of the gap, providing hysteresis even for cards with different heights.
  if (current > 0 && y < centers[current - 1]) return current - 1;
  if (current < heights.length - 1 && y > centers[current + 1]) return current + 1;
  return current;
}
