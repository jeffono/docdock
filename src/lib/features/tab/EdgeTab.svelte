<script lang="ts">
/**
 * The edge tab: a bulge of the panel's inner edge with the app's wordmark, drawn in the strip as
 * deep as the tab that the window keeps beside the panel. One path draws the edge's line, or its
 * focus band, down the panel and around the tab, so no seam shows where they meet. Rust shapes the
 * window to the same outline (`handle::Outline`), so clicks beside the tab reach the windows below.
 */
import { onMount } from "svelte";
import wordmarkSource from "../../../../src-tauri/icons/source/wordmark.svg?raw";
import { panelTab, type TabShape } from "../../ipc";
import { Press } from "./press";

let {
  shape,
  side,
  focused,
  pinned,
}: { shape: TabShape; side: "left" | "right"; focused: boolean; pinned: boolean } = $props();

/** Length of the wordmark along the tab. */
const WORDMARK = 52;
/** How far the edge's line starts above the window and ends below it, so it shows no caps. */
const PAD = 8;

const wordmarkPaths = [...wordmarkSource.matchAll(/\sd="([^"]+)"/g)].map((match) => match[1]);

let height = $state(0);
let ratio = $state(window.devicePixelRatio);
let wordmark = $state<SVGGElement>();
/** The wordmark drawing's tight bounds, measured once it shows. */
let bounds = $state({ x: 0, y: 0, width: 264, height: 50 });

onMount(() => {
  // A move to a monitor with another scale resizes the viewport.
  const resized = () => {
    ratio = window.devicePixelRatio;
  };
  window.addEventListener("resize", resized);
  return () => window.removeEventListener("resize", resized);
});

$effect(() => {
  if (wordmark) bounds = wordmark.getBBox();
});

/** Like the panel's 1 px lines, which Chromium floors to whole device pixels, at least one. */
const line = $derived(Math.max(1, Math.floor(ratio)) / ratio);
/** The 2 px focus band, rounded to device pixels. */
const band = $derived(Math.round(2 * ratio) / ratio);
const stroke = $derived(focused ? band : line);
/** The drawing reaches into the panel by the band, which it draws over the panel's edge. */
const width = $derived(shape.depth + band);
/** Mirrors x for a panel on the left, whose tab grows right; x counts from the tab's outer side. */
const mx = (x: number) => (side === "right" ? x : width - x);
/** Vertically centered, as Rust centers the window's region for it. */
const top = $derived((height - shape.height) / 2);

/** From the panel's edge at the tab's top, out and around it, back to the edge at its bottom. */
const around = $derived.by(() => {
  const [edge, outer] = [mx(shape.depth), mx(0)];
  const { curve } = shape;
  const half = curve / 2;
  const bottom = top + shape.height;
  return [
    `C ${edge} ${top + half} ${outer} ${top + half} ${outer} ${top + curve}`,
    `L ${outer} ${bottom - curve}`,
    `C ${outer} ${bottom - half} ${edge} ${bottom - half} ${edge} ${bottom}`,
  ].join(" ");
});
/** Down the panel's edge, around the tab, and on down the edge. */
const boundary = $derived(
  `M ${mx(shape.depth)} ${-PAD} L ${mx(shape.depth)} ${top} ${around} L ${mx(shape.depth)} ${height + PAD}`,
);
/**
 * The tab, closed a band's width inside the panel: overlapping the panel's background, its edge
 * cannot leave a seam of partly covered pixels where they meet.
 */
const tab = $derived(
  `M ${mx(width)} ${top} L ${mx(shape.depth)} ${top} ${around} L ${mx(width)} ${top + shape.height} Z`,
);
/** Everything inside the boundary: the stroke, twice as wide, keeps only its inner half. */
const inside = $derived(`${boundary} L ${mx(width)} ${height + PAD} L ${mx(width)} ${-PAD} Z`);

/**
 * A quarter turn counterclockwise, reading bottom to top, centered between the band and the
 * panel's edge so focus does not move it.
 */
const placement = $derived.by(() => {
  const scale = WORDMARK / bounds.width;
  const x = mx((shape.depth + band) / 2);
  const y = top + shape.height / 2;
  const [u, v] = [bounds.x + bounds.width / 2, bounds.y + bounds.height / 2];
  return `matrix(0 ${-scale} ${scale} 0 ${x - scale * v} ${y + scale * u})`;
});

/** A press on the tab, until released: only one released on the tab without dragging closes. */
let press: Press | null = null;
const physical = (event: PointerEvent) =>
  [event.screenX * window.devicePixelRatio, event.screenY * window.devicePixelRatio] as const;

function down(event: PointerEvent) {
  // The window's own handler would give the panel focus; clicking the tab never takes it.
  event.stopPropagation();
  if (event.button !== 0) return;
  (event.currentTarget as Element).setPointerCapture(event.pointerId);
  press = new Press(...physical(event));
}

function move(event: PointerEvent) {
  press?.moved(...physical(event), shape.drag);
}

function up(event: PointerEvent) {
  const released = press;
  press = null;
  // The capture delivers a release anywhere: only one on the tab's shape counts, not beside its
  // curves nor on the panel.
  const onTab = document.elementFromPoint(event.clientX, event.clientY) === event.currentTarget;
  if (released && onTab && released.isClick(...physical(event), shape.drag))
    void panelTab().catch(() => {});
}
</script>

<svg
  class="edge"
  class:left={side === "left"}
  class:focused
  class:lit={focused || pinned}
  {width}
  bind:clientHeight={height}
  aria-hidden="true"
>
  <defs>
    <clipPath id="edge-inside"><path d={inside} /></clipPath>
  </defs>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <path
    class="tab"
    d={tab}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onlostpointercapture={() => (press = null)}
  />
  <path class="outline" d={boundary} stroke-width={2 * stroke} clip-path="url(#edge-inside)" />
  <g bind:this={wordmark} transform={placement}>
    {#each wordmarkPaths as d}<path {d} />{/each}
  </g>
</svg>

<style>
  .edge {
    position: fixed;
    z-index: 2;
    top: 0;
    left: 0;
    height: 100%;
    overflow: visible;
    pointer-events: none;
  }

  .edge.left {
    right: 0;
    left: auto;
  }

  .tab {
    fill: var(--color-bg);
    pointer-events: auto;
  }

  .outline {
    fill: none;
    stroke: var(--color-line);
  }

  .focused .outline {
    stroke: var(--color-accent);
  }

  g {
    fill: var(--color-muted);
  }

  .lit g {
    fill: var(--color-fg);
  }
</style>
