<script lang="ts">
// The image preview window: one note image, sized by Rust to the window. It takes no input and has
// no capability; it hears what to show only through its IPC channel. The theme follows the native
// window theme Rust sets.
import { onMount } from "svelte";
import { type ImagePreviewUpdate, imagePreviewState } from "../../ipc";
import { noteAssetUrl } from "../notes/asset-url";
import { imageSwap } from "./image-swap";

let frame: HTMLElement;
let shown = $state(false);

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

/** The answer and the channel may arrive in either order: only a newer generation counts. */
let latest = -1;
function apply(update: ImagePreviewUpdate) {
  if (update.generation <= latest) return;
  latest = update.generation;
  void show(update.path === null ? null : noteAssetUrl(update.path));
}

onMount(() => {
  imagePreviewState(apply)
    .then((state) => {
      document.documentElement.lang = state.locale;
      apply(state);
    })
    .catch(() => {});
});
</script>

<div class="frame" class:shown bind:this={frame}></div>

<style>
  :global(body:has(> .frame)) {
    background: transparent;
  }

  .frame {
    box-sizing: border-box;
    height: 100%;
    overflow: hidden;
  }

  .frame.shown {
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
  }

  .frame :global(img) {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    user-select: none;
  }
</style>
