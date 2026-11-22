<script lang="ts">
// A floating window: one note image detached from the panel, placed by Rust, which keeps it on top
// without ever activating it and keeps the window to the image's proportions. It has no
// capability: it hears what to show only through its IPC channel and reaches only its own
// commands. The image sits in a stage inset by a transparent gutter, where its hover shadow shows
// and the resize strips sit. Its controls show on hover, or while Alt held over a click-through
// window reveals them; the image fades, never the controls. The theme follows the native window
// theme Rust sets.
import { onMount } from "svelte";
import { setLocale, t } from "../../i18n";
import { type FloatUpdate, floatDrag, floatNatural, floatSetOpacity, floatZoom } from "../../ipc";
import { noteAssetUrl } from "../notes/asset-url";
import FloatBar from "./FloatBar.svelte";
import FloatEdges from "./FloatEdges.svelte";
import { latestSender, stepOpacity } from "./opacity";
import type { FloatSession } from "./session";
import { clampPan, type Point, type Size, zoomAt, zoomStep } from "./zoom";

let { session }: { session: FloatSession } = $props();

let root: HTMLElement;
let stage: HTMLElement;
let image: HTMLImageElement;
let locale = $state("en");
/** The image's natural width and the stage's width, in CSS pixels, for the zoom shown. */
let naturalWidth = $state(0);
let stageWidth = $state(0);
let path = $state<string | null>(null);
let opacity = $state(1);
let clickThrough = $state(false);
let revealed = $state(false);
let hovered = $state(false);
/** Zoom inside the window, past the largest the window grows, and where the image is panned. */
let zoom = $state(1);
let pan = $state<Point>({ x: 0, y: 0 });
const controls = $derived(revealed || (hovered && !clickThrough));
const labels = $derived.by(() => {
  void locale;
  return { window: t("float.label"), zoom: t("float.zoom") };
});
const name = $derived(path?.slice(path.lastIndexOf("/") + 1) ?? "");

/** The answer and the channel may arrive in either order: only a newer version counts. */
let latest = -1;
function apply(update: FloatUpdate) {
  if (update.version <= latest) return;
  latest = update.version;
  path = update.path;
  // While the user is still changing it, the slider keeps the latest value they chose.
  if (!opacitySending.busy()) opacity = update.opacity;
  // A window that starts or stops letting clicks through waits for the pointer to come back.
  if (update.clickThrough !== clickThrough) hovered = false;
  clickThrough = update.clickThrough;
  revealed = update.revealed;
}

// Through the CSSOM: the CSP forbids style attributes.
$effect(() => {
  image.style.opacity = String(opacity);
});
$effect(() => {
  image.style.transform = zoom === 1 ? "" : `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`;
});
$effect(() => {
  if (path === null) image.removeAttribute("src");
  else image.src = noteAssetUrl(path);
});

const opacitySending = latestSender(floatSetOpacity);
function setOpacity(value: number) {
  opacity = value;
  void opacitySending.send(value);
}

const stageSize = (): Size => ({ width: stage.clientWidth, height: stage.clientHeight });
/** The image's size against its natural size, zoom inside included, as a whole percent. */
const percent = $derived(
  naturalWidth > 0 && stageWidth > 0 ? Math.round((stageWidth / naturalWidth) * zoom * 100) : 0,
);
const naturalSize = (): Size => ({ width: image.naturalWidth, height: image.naturalHeight });

/** The window is as large as it gets; any resize makes Rust say so again. */
let atMax = false;
let resizing = false;
function zoomInside(next: number, at: Point) {
  pan = zoomAt(pan, zoom, next, at, stageSize());
  zoom = next;
}
async function zoomBy(zoomIn: boolean, at: Point) {
  const step = zoomStep(stageSize(), naturalSize(), zoom, atMax, zoomIn);
  if (step.kind === "inner") return zoomInside(step.zoom, at);
  if (resizing) return;
  resizing = true;
  try {
    const before = stageSize().width;
    const zoomed = await floatZoom(step.width);
    atMax = zoomed.atMax;
    // Already as large as it gets: this notch zooms inside instead.
    if (zoomIn && zoomed.atMax && Math.abs(zoomed.width - before) < 1) {
      const next = zoomStep(stageSize(), naturalSize(), zoom, true, true);
      if (next.kind === "inner") zoomInside(next.zoom, at);
    }
  } catch {
    // A window closing meanwhile.
  } finally {
    resizing = false;
  }
}

function fromCenter(event: MouseEvent): Point {
  const box = stage.getBoundingClientRect();
  return {
    x: event.clientX - box.left - box.width / 2,
    y: event.clientY - box.top - box.height / 2,
  };
}

/** Ctrl and Alt together are AltGr on layouts such as ABNT2: they do nothing here. */
const altGr = (event: MouseEvent) => event.ctrlKey && event.altKey;

/** Ctrl+wheel zooms, Alt+wheel fades; so does the plain wheel over the bar. */
function wheel(event: WheelEvent) {
  if (event.deltaY === 0 || altGr(event)) return;
  const up = event.deltaY < 0;
  if (event.ctrlKey && !event.altKey) {
    event.preventDefault();
    void zoomBy(up, fromCenter(event));
    return;
  }
  const overBar = controls && (event.target as Element).closest(".bar") !== null;
  if ((event.altKey && !event.ctrlKey) || overBar) {
    event.preventDefault();
    setOpacity(stepOpacity(opacity, up));
  }
}

/**
 * Ctrl+drag pans an image zoomed inside the window; any other drag moves the window. Ctrl never
 * moves it: the native move loop would take the second click of a Ctrl+double-click.
 */
let panning: { pointer: number; from: Point; start: Point } | null = null;
function pointerdown(event: PointerEvent) {
  if (event.button !== 0 || (event.target as Element).closest("button, input, .edge")) return;
  if (altGr(event)) return;
  event.preventDefault();
  if (event.ctrlKey) {
    if (zoom <= 1) return;
    root.setPointerCapture(event.pointerId);
    panning = {
      pointer: event.pointerId,
      from: { x: event.clientX, y: event.clientY },
      start: pan,
    };
    return;
  }
  void floatDrag().catch(() => {});
}
function pointermove(event: PointerEvent) {
  hovered = true;
  if (!panning || event.pointerId !== panning.pointer) return;
  const moved = {
    x: panning.start.x + event.clientX - panning.from.x,
    y: panning.start.y + event.clientY - panning.from.y,
  };
  pan = clampPan(moved, zoom, stageSize());
}
function pointerup(event: PointerEvent) {
  if (panning?.pointer === event.pointerId) panning = null;
}

/** Ctrl+double-click returns to the whole image, at its natural size where it fits. */
async function reset(event: MouseEvent) {
  if (!event.ctrlKey || event.altKey) return;
  if ((event.target as Element).closest("button, input, .edge")) return;
  event.preventDefault();
  zoom = 1;
  pan = { x: 0, y: 0 };
  const natural = naturalSize();
  if (natural.width === 0) return;
  atMax = false;
  try {
    atMax = (await floatZoom(natural.width)).atMax;
  } catch {
    // A window closing meanwhile.
  }
}

onMount(() => {
  locale = session.state.locale;
  setLocale(session.state.locale);
  apply(session.state);
  session.listen(apply);
  // Not passive: Ctrl+wheel would otherwise zoom the page itself.
  root.addEventListener("wheel", wheel, { passive: false });
  const resized = () => {
    stageWidth = stage.clientWidth;
    atMax = false;
    pan = clampPan(pan, zoom, stageSize());
  };
  window.addEventListener("resize", resized);
  return () => {
    root.removeEventListener("wheel", wheel);
    window.removeEventListener("resize", resized);
    image.removeAttribute("src");
  };
});
</script>

{#snippet zoomLabel()}<span class="zoom" title={labels.zoom}>{percent}%</span>{/snippet}

<div
  class="float"
  class:active={controls}
  bind:this={root}
  role="group"
  aria-label={labels.window}
  onpointerenter={() => { hovered = true; }}
  onpointermove={pointermove}
  onpointerup={pointerup}
  onpointercancel={pointerup}
  onpointerleave={() => { hovered = false; }}
  onpointerdown={pointerdown}
  ondblclick={reset}
>
  <div class="stage" bind:this={stage}>
    <img
      bind:this={image}
      alt={name}
      draggable="false"
      onload={() => {
        naturalWidth = image.naturalWidth;
        stageWidth = stage.clientWidth;
        void floatNatural(image.naturalWidth, image.naturalHeight).catch(() => {});
      }}
    />
  </div>
  <FloatBar
    shown={controls}
    {opacity}
    {clickThrough}
    {locale}
    onopacity={setOpacity}
    before={percent > 0 ? zoomLabel : undefined}
  />
  {#if !clickThrough || revealed}<FloatEdges />{/if}
</div>

<style>
  :global(body:has(> .float)) {
    background: transparent;
  }

  /* `--pad` is the transparent gutter around the image, `SHADOW_PAD` in `floats.rs`. */
  .float {
    --pad: var(--space-2);
    position: fixed;
    inset: 0;
  }

  .stage {
    position: absolute;
    inset: var(--pad);
    overflow: hidden;
    transition: filter var(--duration);
  }

  /* Traces the image's own edges, or a transparent image's outline, never a frame of its own. */
  .float.active .stage {
    filter: drop-shadow(0 0 1px var(--color-float-ring))
      drop-shadow(0 1px var(--space-1) var(--color-float-shadow));
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    /* The window keeps the image's proportions; until Rust has them, nothing is cropped. */
    object-fit: contain;
    transform-origin: center;
    user-select: none;
  }

  /* How large the image shows against its natural size, inside zoom included. */
  .zoom {
    margin: 0 var(--space-1);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    font-variant-numeric: tabular-nums;
    user-select: none;
  }

  /* A narrow window keeps room for toggling and closing. */
  @media (max-width: 12rem) {
    .zoom {
      display: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .stage {
      transition: none;
    }
  }
</style>
