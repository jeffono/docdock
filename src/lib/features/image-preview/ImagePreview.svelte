<script lang="ts">
// The image preview window: one note image in a frame with an arrow toward its thumbnail, sized
// and placed by Rust. It takes no input and has no capability; it hears what to show only through
// its IPC channel. The theme follows the native window theme Rust sets.
import { onMount } from "svelte";
import { type ImagePreviewUpdate, imagePreviewState } from "../../ipc";
import { noteAssetUrl } from "../notes/asset-url";
import { bubblePath } from "./bubble";
import { imageSwap } from "./image-swap";

let frame: HTMLElement;
let shown = $state(false);
let side = $state<"left" | "right">("right");
let arrowY = $state(0);
let width = $state(0);
let height = $state(0);
let ratio = $state(window.devicePixelRatio);

/** Like the panel's 1 px lines, which Chromium floors to whole device pixels, at least one. */
const line = $derived(Math.max(1, Math.floor(ratio)) / ratio);
const outline = $derived(bubblePath(width, height, arrowY, line / 2));
/** The left side's frame is the right side's, mirrored. */
const mirror = $derived(side === "left" ? `matrix(-1 0 0 1 ${width} 0)` : undefined);

const show = imageSwap(
  () => {
    const image = new Image();
    image.alt = "";
    image.draggable = false;
    return image;
  },
  (image) => {
    frame.replaceChildren(image);
    shown = true;
  },
  () => {
    for (const image of frame.querySelectorAll("img")) image.removeAttribute("src");
    frame.replaceChildren();
    shown = false;
  },
);

/** The answer and the channel may arrive in either order: only a newer version counts. */
let latest = -1;
let path: string | null = null;
function apply(update: ImagePreviewUpdate) {
  if (update.version <= latest) return;
  latest = update.version;
  side = update.side;
  arrowY = update.arrowY;
  // A new placement for the same image only moves the arrow.
  if (update.path === path) return;
  path = update.path;
  void show(path === null ? null : noteAssetUrl(path));
}

onMount(() => {
  // A move to a monitor with another scale resizes the viewport.
  const resized = () => {
    ratio = window.devicePixelRatio;
  };
  window.addEventListener("resize", resized);
  imagePreviewState(apply)
    .then((state) => {
      document.documentElement.lang = state.locale;
      apply(state);
    })
    .catch(() => {});
  return () => window.removeEventListener("resize", resized);
});
</script>

<svg class="bubble" class:shown bind:clientWidth={width} bind:clientHeight={height} aria-hidden="true">
  <path d={outline} transform={mirror} stroke-width={line} />
</svg>
<div class="frame" class:shown class:side-left={side === "left"} class:side-right={side === "right"} bind:this={frame}></div>

<style>
  :global(body:has(> .frame)) {
    background: transparent;
  }

  .bubble,
  .frame {
    position: fixed;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .bubble {
    visibility: hidden;
  }

  .bubble.shown {
    visibility: visible;
  }

  path {
    fill: var(--color-bg);
    stroke: var(--color-line);
    stroke-linejoin: round;
  }

  /* The padding of the frame, and on the arrow's side its depth too, which is as deep. */
  .frame {
    box-sizing: border-box;
    padding: var(--space-2);
  }

  .frame.side-right {
    padding-right: calc(2 * var(--space-2));
  }

  .frame.side-left {
    padding-left: calc(2 * var(--space-2));
  }

  .frame :global(img) {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    border-radius: var(--radius-sm);
    user-select: none;
  }
</style>
