<script lang="ts">
import { type Curve, limits } from "./motion";

let { curve, label, onchange }: { curve: Curve; label: string; onchange: (curve: Curve) => void } =
  $props();

// The unit square spans 0..100; y grows upward and the view leaves room to overshoot.
const view = { low: -0.5, high: 1.5 };
const px = (x: number) => x * 100;
const py = (y: number) => 100 - y * 100;
let svg: SVGSVGElement;
let dragging = -1;

function move(event: PointerEvent) {
  if (dragging < 0) return;
  const matrix = svg.getScreenCTM()?.inverse();
  if (!matrix) return;
  const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix);
  const round = (value: number) => Math.round(value * 100) / 100;
  const x = round(Math.min(limits.x[1], Math.max(limits.x[0], point.x / 100)));
  const y = round(Math.min(view.high, Math.max(view.low, (100 - point.y) / 100)));
  const next = [...curve] as Curve;
  next[dragging * 2] = x;
  next[dragging * 2 + 1] = y;
  onchange(next);
}
function start(event: PointerEvent, index: number) {
  dragging = index;
  (event.currentTarget as Element).setPointerCapture(event.pointerId);
}
</script>

<svg bind:this={svg} viewBox="-6 -56 112 212" role="img" aria-label={label} onpointermove={move} onpointerup={() => { dragging = -1; }} onlostpointercapture={() => { dragging = -1; }}>
  <rect class="square" x="0" y="0" width="100" height="100" />
  <line class="arm" x1="0" y1="100" x2={px(curve[0])} y2={py(curve[1])} />
  <line class="arm" x1="100" y1="0" x2={px(curve[2])} y2={py(curve[3])} />
  <path class="curve" d={`M0 100 C ${px(curve[0])} ${py(curve[1])}, ${px(curve[2])} ${py(curve[3])}, 100 0`} />
  {#each [0, 1] as index}
    <!-- The x1..y2 fields next to the editor are the accessible way to set the points. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <circle class="point" cx={px(curve[index * 2])} cy={py(curve[index * 2 + 1])} r="5" onpointerdown={(e) => start(e, index)} />
  {/each}
</svg>

<style>
  svg { display: block; width: 100%; max-width: calc(var(--size-bar) * 3); touch-action: none; }
  .square { fill: var(--color-surface); stroke: var(--color-line); stroke-width: 1; }
  .arm { stroke: var(--color-muted); stroke-width: 1; }
  .curve { fill: none; stroke: var(--color-fg); stroke-width: 2; }
  .point { fill: var(--color-accent); cursor: grab; }
</style>
