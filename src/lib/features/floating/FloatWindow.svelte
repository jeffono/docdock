<script lang="ts">
// A floating window: one note image detached from the panel, placed by Rust, which keeps it on top
// without ever activating it. It has no capability: it hears what to show only through its IPC
// channel and reaches only its own commands. Its controls show on hover, or while Alt held over a
// click-through window reveals them; the image fades, never the controls. The theme follows the
// native window theme Rust sets.
import { onMount } from "svelte";
import { setLocale, t } from "../../i18n";
import {
  type FloatUpdate,
  floatClose,
  floatDrag,
  floatResize,
  floatSetClickThrough,
  floatSetOpacity,
  floatState,
  type ResizeDirection,
} from "../../ipc";
import IconButton from "../../ui/IconButton.svelte";
import { noteAssetUrl } from "../notes/asset-url";
import { MIN_OPACITY, OPACITY_STEP, stepOpacity } from "./opacity";

let image: HTMLImageElement;
let locale = $state("en");
let path = $state<string | null>(null);
let opacity = $state(1);
let clickThrough = $state(false);
let revealed = $state(false);
let hovered = $state(false);
const controls = $derived(revealed || (hovered && !clickThrough));
const labels = $derived.by(() => {
  void locale;
  return {
    window: t("float.label"),
    opacity: t("float.opacity"),
    clickThrough: t("float.clickThrough"),
    close: t("float.close"),
  };
});
const name = $derived(path?.slice(path.lastIndexOf("/") + 1) ?? "");

/** The answer and the channel may arrive in either order: only a newer version counts. */
let latest = -1;
function apply(update: FloatUpdate) {
  if (update.version <= latest) return;
  latest = update.version;
  path = update.path;
  // While the user is still changing it, the slider keeps the latest value they chose.
  if (pending === null && !sending) opacity = update.opacity;
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
  if (path === null) image.removeAttribute("src");
  else image.src = noteAssetUrl(path);
});

/** One change in flight, the latest queued: answers may otherwise land out of order. */
let pending: number | null = null;
let sending = false;
async function setOpacity(value: number) {
  opacity = value;
  pending = value;
  if (sending) return;
  sending = true;
  while (pending !== null) {
    const next = pending;
    pending = null;
    await floatSetOpacity(next).catch(() => {});
  }
  sending = false;
}

function wheel(event: WheelEvent) {
  // Over the bar the wheel fades the image; elsewhere it needs Ctrl.
  if (!controls || (!event.ctrlKey && !(event.target as Element).closest(".bar"))) return;
  event.preventDefault();
  if (event.deltaY !== 0) void setOpacity(stepOpacity(opacity, event.deltaY < 0));
}

/** The image and the bar's background move the window; controls and edges do their own thing. */
function drag(event: PointerEvent) {
  if (event.button !== 0 || (event.target as Element).closest("button, input, .edge")) return;
  event.preventDefault();
  void floatDrag().catch(() => {});
}

function resize(event: PointerEvent, direction: ResizeDirection) {
  if (event.button !== 0) return;
  event.preventDefault();
  void floatResize(direction).catch(() => {});
}

const edges: [string, ResizeDirection][] = [
  ["n", "North"],
  ["s", "South"],
  ["e", "East"],
  ["w", "West"],
  ["ne", "NorthEast"],
  ["nw", "NorthWest"],
  ["se", "SouthEast"],
  ["sw", "SouthWest"],
];

onMount(() => {
  floatState(apply)
    .then((state) => {
      locale = state.locale;
      setLocale(state.locale);
      apply(state);
    })
    .catch(() => {});
  return () => image.removeAttribute("src");
});
</script>

<div
  class="float"
  class:active={controls}
  role="group"
  aria-label={labels.window}
  onpointerenter={() => { hovered = true; }}
  onpointermove={() => { hovered = true; }}
  onpointerleave={() => { hovered = false; }}
  onpointerdown={drag}
  onwheel={wheel}
>
  <img bind:this={image} alt={name} draggable="false" />
  <div class="bar" class:shown={controls}>
    <input
      type="range"
      min={MIN_OPACITY * 100}
      max="100"
      step={OPACITY_STEP * 100}
      value={Math.round(opacity * 100)}
      aria-label={labels.opacity}
      title={`${labels.opacity}: ${Math.round(opacity * 100)}%`}
      aria-valuetext={`${Math.round(opacity * 100)}%`}
      tabindex="-1"
      oninput={(event) => void setOpacity(Number(event.currentTarget.value) / 100)}
    />
    <IconButton
      icon="clickThrough"
      label={labels.clickThrough}
      pressed={clickThrough}
      tabindex={-1}
      onclick={() => void floatSetClickThrough(!clickThrough).catch(() => {})}
    />
    <IconButton icon="x" quiet label={labels.close} tabindex={-1} onclick={() => void floatClose().catch(() => {})} />
  </div>
  {#if !clickThrough || revealed}
    {#each edges as [edge, direction] (edge)}
      <div class="edge {edge}" aria-hidden="true" onpointerdown={(event) => resize(event, direction)}></div>
    {/each}
  {/if}
</div>

<style>
  :global(body:has(> .float)) {
    background: transparent;
  }

  .float {
    position: fixed;
    inset: 0;
    border-radius: var(--radius-sm);
  }

  /* Shows where a faded window ends while its controls are on. */
  .float.active {
    outline: 1px solid var(--color-line);
    outline-offset: -1px;
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    user-select: none;
  }

  .bar {
    position: absolute;
    top: var(--space-1);
    right: var(--space-1);
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    visibility: hidden;
    opacity: 0;
    transition:
      opacity var(--duration),
      visibility var(--duration);
  }

  .bar.shown {
    visibility: visible;
    opacity: 1;
  }

  input[type="range"] {
    width: calc(3 * var(--size-control));
    margin: 0 var(--space-1);
    accent-color: var(--color-accent);
  }

  /* A narrow window keeps room for toggling and closing. */
  @media (max-width: 12rem) {
    input[type="range"] {
      display: none;
    }
  }

  /* Invisible strips along the borderless window's edges and corners, where it resizes. */
  .float {
    --edge: calc(1.5 * var(--space-1));
  }

  .edge {
    position: absolute;
  }

  .n,
  .s {
    left: var(--edge);
    right: var(--edge);
    height: var(--edge);
    cursor: ns-resize;
  }

  .e,
  .w {
    top: var(--edge);
    bottom: var(--edge);
    width: var(--edge);
    cursor: ew-resize;
  }

  .ne,
  .nw,
  .se,
  .sw {
    width: var(--edge);
    height: var(--edge);
  }

  .n,
  .ne,
  .nw {
    top: 0;
  }

  .s,
  .se,
  .sw {
    bottom: 0;
  }

  .e,
  .ne,
  .se {
    right: 0;
  }

  .w,
  .nw,
  .sw {
    left: 0;
  }

  .ne,
  .sw {
    cursor: nesw-resize;
  }

  .nw,
  .se {
    cursor: nwse-resize;
  }

  @media (prefers-reduced-motion: reduce) {
    .bar {
      transition: none;
    }
  }
</style>
