<script lang="ts">
// The corner button that detaches the note image under the pointer, or focused, into a floating
// window. One button for the whole list, laid over the image, so the rendered note stays as it is.
import { t } from "../../i18n";
import IconButton from "../../ui/IconButton.svelte";
import { imageTarget, openFloating } from "./detach";

let {
  root,
  off = false,
  onerror,
}: {
  /** The list whose note images it serves. */
  root: HTMLElement | undefined;
  /** Hidden, such as while Browser or Preferences show. */
  off?: boolean;
  onerror: (error: unknown) => void;
} = $props();

let corner: HTMLElement;
let target = $state<HTMLElement | null>(null);

function show(next: HTMLElement | null) {
  target = next;
  if (!next) return;
  const box = next.getBoundingClientRect();
  // Through the CSSOM: the CSP forbids style attributes.
  corner.style.left = `${box.right}px`;
  corner.style.top = `${box.top}px`;
}

$effect(() => {
  if (off) target = null;
});

$effect(() => {
  const list = root;
  if (!list) return;
  const over = (event: PointerEvent) => show(imageTarget(event.target));
  const leave = (event: PointerEvent) => {
    if (!(event.relatedTarget instanceof Node && corner.contains(event.relatedTarget))) show(null);
  };
  const focus = (event: FocusEvent) => {
    const image = imageTarget(event.target);
    if (image) show(image);
  };
  const blur = (event: FocusEvent) => {
    if (event.target === target) show(null);
  };
  const hide = () => show(null);
  list.addEventListener("pointerover", over);
  list.addEventListener("pointerleave", leave);
  list.addEventListener("focusin", focus);
  list.addEventListener("focusout", blur);
  list.addEventListener("scroll", hide, { passive: true });
  window.addEventListener("resize", hide);
  return () => {
    list.removeEventListener("pointerover", over);
    list.removeEventListener("pointerleave", leave);
    list.removeEventListener("focusin", focus);
    list.removeEventListener("focusout", blur);
    list.removeEventListener("scroll", hide);
    window.removeEventListener("resize", hide);
  };
});

function detach() {
  const image = target;
  show(null);
  // A note refreshed under a resting pointer replaced the image it was shown for.
  if (image?.isConnected) void openFloating(image).catch(onerror);
}
</script>

<div
  class="detach"
  class:shown={target !== null && !off}
  bind:this={corner}
  role="presentation"
  onpointerleave={(event) => {
    if (!(event.relatedTarget instanceof Node && target?.contains(event.relatedTarget))) show(null);
  }}
>
  <IconButton icon="pictureInPicture" label={t("image.float")} tabindex={-1} onclick={detach} />
</div>

<style>
  /* At the image's top right corner, inside it, over the image's own surface. */
  .detach {
    position: fixed;
    display: none;
    margin-top: var(--space-1);
    transform: translateX(calc(-100% - var(--space-1)));
    border-radius: var(--radius-sm);
    background: var(--color-surface);
  }

  .detach.shown {
    display: block;
  }
</style>
