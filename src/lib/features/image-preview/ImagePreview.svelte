<script lang="ts">
// The side preview window: one note image, closed markdown note callout, or opaque post-it drag
// ghost placed by Rust. It takes no input and has no capability; it hears what to show only
// through its IPC channel. The theme follows the native window theme Rust sets.
import { onMount } from "svelte";
import { setLocale, t } from "../../i18n";
import { type ImagePreviewUpdate, imagePreviewNote, imagePreviewState } from "../../ipc";
import { noteAssetUrl } from "../notes/asset-url";
import { renderNote } from "../notes/render";
import "../notes/content.css";
import { bubblePath } from "./bubble";
import { imageSwap } from "./image-swap";
import { clipPreviewNote } from "./note-clip";

const MIN_BUBBLE_H = 56;
const FRAME_PAD = 8;
const CLIP_PAD_Y = 4;
const REACH = 18;

const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value));

let frame: HTMLElement;
let shown = $state(false);
let kind = $state<"image" | "note" | "postit">("image");
let side = $state<"left" | "right">("right");
let arrowY = $state(0);
let width = $state(0);
let height = $state(0);
let noteH = $state<number | null>(null);
let ratio = $state(window.devicePixelRatio);

const bubbleH = $derived(
  kind === "note" && noteH !== null && height > 0
    ? clamp(noteH, Math.min(MIN_BUBBLE_H, height), height)
    : height,
);
const bubbleTop = $derived(
  kind === "note" && noteH !== null && height > 0
    ? clamp(Math.round(arrowY - bubbleH / 2), 0, Math.max(0, height - bubbleH))
    : 0,
);
const localArrowY = $derived(
  kind === "note" && bubbleH > 0
    ? clamp(
        arrowY - bubbleTop,
        Math.min(REACH, bubbleH / 2),
        Math.max(Math.min(REACH, bubbleH / 2), bubbleH - Math.min(REACH, bubbleH / 2)),
      )
    : arrowY,
);

/** Like the panel's 1 px lines, which Chromium floors to whole device pixels, at least one. */
const line = $derived(Math.max(1, Math.floor(ratio)) / ratio);
const outline = $derived(bubblePath(width, bubbleH, localArrowY, line / 2));
/** The left side's frame is the right side's, mirrored; shifted when a note bubble is compact. */
const transform = $derived(
  side === "left"
    ? `matrix(-1 0 0 1 ${width} ${bubbleTop})`
    : bubbleTop !== 0
      ? `translate(0 ${bubbleTop})`
      : undefined,
);

// Through the CSSOM: the CSP forbids style attributes.
$effect(() => {
  if (!frame) return;
  if (kind === "note" && noteH !== null && height > 0) {
    frame.style.top = `${bubbleTop}px`;
    frame.style.height = `${bubbleH}px`;
    const clip = frame.querySelector<HTMLElement>(".note-clip");
    if (clip) clip.classList.toggle("clipped", clip.scrollHeight > clip.clientHeight + 1);
  } else {
    frame.style.top = "";
    frame.style.height = "";
  }
});

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
let noteRead = 0;
let stopMeasure: (() => void) | null = null;

function clearNote() {
  stopMeasure?.();
  stopMeasure = null;
  noteH = null;
  if (frame) {
    frame.style.top = "";
    frame.style.height = "";
  }
}

function mountNote(element: HTMLElement) {
  clearNote();
  const clip = document.createElement("div");
  clip.className = "note-clip";
  clip.append(element);
  frame.replaceChildren(clip);
  const measure = () => {
    const contentHeight = Math.ceil(element.getBoundingClientRect().height);
    noteH = contentHeight + 2 * FRAME_PAD + 2 * CLIP_PAD_Y;
    clip.classList.toggle("clipped", clip.scrollHeight > clip.clientHeight + 1);
  };
  const observer = new ResizeObserver(measure);
  observer.observe(element);
  observer.observe(clip);
  measure();
  const raf = requestAnimationFrame(measure);
  stopMeasure = () => {
    cancelAnimationFrame(raf);
    observer.disconnect();
  };
}

async function showNote(notePath: string, mine: number) {
  const raw = await imagePreviewNote().catch(() => null);
  if (mine !== noteRead || raw === null) return;
  const trimmed = clipPreviewNote(raw);
  let node: HTMLElement;
  if (!trimmed.trim()) {
    const empty = document.createElement("p");
    empty.className = "preview-empty";
    empty.textContent = t("preview.emptyNote");
    node = empty;
  } else {
    const fragment = await renderNote(trimmed, notePath);
    if (mine !== noteRead) return;
    const body = document.createElement("div");
    body.className = "note-content note-preview";
    body.append(fragment);
    for (const stop of body.querySelectorAll<HTMLElement>("a, .image-thumb")) stop.tabIndex = -1;
    node = body;
  }
  mountNote(node);
  shown = true;
}

async function showPostIt(notePath: string, mine: number) {
  const raw = await imagePreviewNote().catch(() => null);
  if (mine !== noteRead || raw === null) return;
  const trimmed = clipPreviewNote(raw);
  const ghost = document.createElement("div");
  ghost.className = "postit-ghost";
  const sheet = document.createElement("div");
  sheet.className = "sheet";
  const scroller = document.createElement("div");
  scroller.className = "scroller";
  const body = document.createElement("div");
  body.className = "body note-content";
  if (trimmed.trim()) {
    const fragment = await renderNote(trimmed, notePath);
    if (mine !== noteRead) return;
    body.append(fragment);
    for (const stop of body.querySelectorAll<HTMLElement>("a, button, input, .image-thumb"))
      stop.tabIndex = -1;
  }
  scroller.append(body);
  sheet.append(scroller);
  ghost.append(sheet);
  frame.replaceChildren(ghost);
  shown = true;
}

function apply(update: ImagePreviewUpdate) {
  if (update.version <= latest) return;
  latest = update.version;
  side = update.side;
  arrowY = update.arrowY;
  // A new placement for the same target only moves the arrow.
  if (update.kind === kind && update.path === path) return;
  const wasMarkdown = kind === "note" || kind === "postit";
  kind = update.kind;
  path = update.path;
  noteRead++;
  clearNote();
  if (path === null) {
    void show(null);
    return;
  }
  if (kind === "image") {
    if (wasMarkdown) {
      frame.replaceChildren();
      shown = false;
    }
    void show(noteAssetUrl(path));
    return;
  }
  const mine = noteRead;
  void show(null);
  if (kind === "postit") {
    void showPostIt(path, mine);
  } else {
    void showNote(path, mine);
  }
}

onMount(() => {
  // A move to a monitor with another scale resizes the viewport.
  const resized = () => {
    ratio = window.devicePixelRatio;
  };
  window.addEventListener("resize", resized);
  imagePreviewState(apply)
    .then((state) => {
      setLocale(state.locale);
      apply(state);
    })
    .catch(() => {});
  return () => {
    clearNote();
    window.removeEventListener("resize", resized);
  };
});
</script>

<svg class="bubble" class:shown={shown && kind !== "postit"} bind:clientWidth={width} bind:clientHeight={height} aria-hidden="true">
  <path d={outline} {transform} stroke-width={line} />
</svg>
<div class="frame" class:note-frame={kind === "note"} class:postit-frame={kind === "postit"} class:shown class:side-left={side === "left"} class:side-right={side === "right"} bind:this={frame}></div>

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

  .frame.note-frame,
  .frame.postit-frame {
    pointer-events: none;
    user-select: none;
  }

  .frame.postit-frame {
    padding: 0;
  }

  .frame > :global(img) {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    border-radius: var(--radius-sm);
    user-select: none;
  }

  .frame :global(.note-clip) {
    box-sizing: border-box;
    height: 100%;
    overflow: hidden;
    padding: var(--space-1) var(--space-2);
  }

  .frame :global(.note-clip.clipped) {
    mask-image: linear-gradient(to bottom, black calc(100% - 2.5em), transparent);
  }

  .frame :global(.note-preview) {
    font-size: var(--font-size-md);
    overflow-wrap: anywhere;
  }

  .frame :global(.note-preview > :first-child) {
    margin-top: 0;
  }

  .frame :global(.note-preview > :last-child) {
    margin-bottom: 0;
  }

  .frame :global(.note-clip .image-grid) {
    grid-template-columns: repeat(auto-fill, minmax(calc(2 * var(--size-control)), 1fr));
    gap: var(--space-1);
  }

  .frame :global(.preview-empty) {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }

  .frame :global(.postit-ghost) {
    --pad: var(--space-2);
    position: relative;
    width: 100%;
    height: 100%;
  }

  .frame :global(.postit-ghost .sheet) {
    position: absolute;
    inset: var(--pad);
    overflow: hidden;
    padding: var(--space-2) 0;
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-fg);
    filter: drop-shadow(0 0 1px var(--color-float-ring))
      drop-shadow(0 1px var(--space-1) var(--color-float-shadow));
  }

  .frame :global(.postit-ghost .scroller) {
    height: 100%;
    overflow: hidden;
    padding: 0 var(--space-3);
  }

  .frame :global(.postit-ghost .body) {
    font-size: var(--font-size-md);
    overflow-wrap: anywhere;
  }

  .frame :global(.postit-ghost .body > :first-child) {
    margin-top: 0;
  }

  .frame :global(.postit-ghost .body > :last-child) {
    margin-bottom: 0;
  }

  .frame :global(.postit-ghost .body .note-header) {
    padding-bottom: var(--space-2);
  }
</style>
