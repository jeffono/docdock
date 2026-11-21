<script lang="ts">
// The image preview window: one note image, sized by Rust to the window. It takes no input, so it
// has nothing but the image. The theme follows the native window theme Rust sets.
import { onMount } from "svelte";
import { imagePreviewState, onImagePreview } from "../../ipc";
import { noteAssetUrl } from "../notes/asset-url";

let frame: HTMLElement;
let shown = $state(false);
/** Changes on each new path, so a slower decode never replaces a later image. */
let turn = 0;

async function show(path: string | null) {
  const current = ++turn;
  if (path === null) {
    clear();
    return;
  }
  // Decoded before the swap, so switching thumbnails never flashes an empty frame.
  const image = new Image();
  image.alt = "";
  image.draggable = false;
  image.src = noteAssetUrl(path);
  try {
    await image.decode();
  } catch {
    if (current === turn) clear();
    return;
  }
  if (current !== turn) return;
  frame.replaceChildren(image);
  shown = true;
}

/** No note image stays in memory once hidden (decision 0009). */
function clear() {
  for (const image of frame.querySelectorAll("img")) image.removeAttribute("src");
  frame.replaceChildren();
  shown = false;
}

onMount(() => {
  // Listening starts before the query, and a path sent meanwhile wins over its older answer.
  let heard = false;
  const stop = onImagePreview((path) => {
    heard = true;
    void show(path);
  });
  void stop
    .then(() => imagePreviewState())
    .then((state) => {
      document.documentElement.lang = state.locale;
      if (!heard) void show(state.path);
    })
    .catch(() => {});
  return () => void stop.then((unlisten) => unlisten());
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
