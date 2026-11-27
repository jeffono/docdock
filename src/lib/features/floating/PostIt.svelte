<script lang="ts">
// A post-it: one note shown read-only in a floating window, placed by Rust, which keeps it on top
// without ever activating it. It has no capability: it hears what to show through its IPC
// channel, reads its note through its own command whenever Rust says the note may have changed,
// and reaches only its own commands. The note sits on a sheet inset by a transparent gutter, where
// its hover shadow shows and the resize strips sit. Its controls show on hover, or while Alt held
// over a click-through window reveals them; the sheet fades, never the controls. Editing happens
// in the panel: a double click, or the pencil, opens the note there.
import { onMount } from "svelte";
import { setLocale, t } from "../../i18n";
import {
  type FloatUpdate,
  floatDrag,
  floatEdit,
  floatNote,
  floatReady,
  floatSetCollapsed,
  floatSetOpacity,
  floatSetTextScale,
} from "../../ipc";
import { renderNote } from "../notes/render";
import "../notes/content.css";
import FloatEdges from "./FloatEdges.svelte";
import { latestSender, stepOpacity } from "./opacity";
import type { FloatSession } from "./session";
import TitleBar from "./TitleBar.svelte";
import { stepTextScale } from "./text-scale";

let { session }: { session: FloatSession } = $props();

let windowRoot: HTMLElement;
let sheet: HTMLElement;
let body: HTMLElement;
let scroller: HTMLElement;
let locale = $state("en");
let path = $state<string | null>(null);
let opacity = $state(1);
let textScale = $state(1);
let clickThrough = $state(false);
let revealed = $state(false);
let collapsed = $state(false);
let hovered = $state(false);
/** The note is gone, as after a rename the post-it did not see: only that shows, until it returns. */
let gone = $state(false);
const controls = $derived(revealed || (hovered && !clickThrough));
const labels = $derived.by(() => {
  void locale;
  return { window: t("postIt.label"), gone: t("postIt.gone") };
});
const name = $derived(path?.slice(path.lastIndexOf("/") + 1).replace(/\.md$/i, "") ?? "");

/** The answer and the channel may arrive in either order: only a newer version counts. */
let latest = -1;
let revision = -1;
function apply(update: FloatUpdate) {
  if (update.version <= latest) return;
  latest = update.version;
  path = update.path;
  // While the user is still changing them, the controls keep the latest values they chose.
  if (!opacitySending.busy()) opacity = update.opacity;
  if (!scaleSending.busy()) textScale = update.textScale;
  // A window that starts or stops letting clicks through waits for the pointer to come back.
  if (update.clickThrough !== clickThrough) hovered = false;
  clickThrough = update.clickThrough;
  revealed = update.revealed;
  collapsed = update.collapsed;
  if (update.revision !== revision) {
    revision = update.revision;
    void read();
  }
}

/**
 * Reads the note again; only the latest read counts, and the same text at the same path is not
 * drawn again (a moved note's relative images resolve anew).
 */
let reads = 0;
let signaledReady = false;
let shown: { text: string; path: string | null } | null = null;
function markReady() {
  if (signaledReady) return;
  signaledReady = true;
  requestAnimationFrame(() => {
    void floatReady().catch(() => {});
  });
}
async function read() {
  const current = ++reads;
  let text: string | null;
  try {
    text = (await floatNote())?.text ?? null;
  } catch {
    // A window closing, or its folder leaving, meanwhile.
    return;
  }
  if (current !== reads) return;
  gone = text === null;
  const at = path;
  if (text === null) {
    markReady();
    return;
  }
  if (text === shown?.text && at === shown.path) {
    markReady();
    return;
  }
  const fragment = await renderNote(text, at);
  if (current !== reads) return;
  shown = { text, path: at };
  body.replaceChildren(fragment);
  // Nothing in a post-it takes focus or acts: it never activates, and editing is the panel's.
  for (const element of body.querySelectorAll<HTMLElement>("a, button, input"))
    element.tabIndex = -1;
  markReady();
}

// Through the CSSOM: the CSP forbids style attributes.
$effect(() => {
  windowRoot.style.setProperty("--sheet-opacity", String(opacity));
});
$effect(() => {
  body.style.zoom = String(textScale);
});

const opacitySending = latestSender(floatSetOpacity);
function setOpacity(value: number) {
  opacity = value;
  void opacitySending.send(value);
}
const scaleSending = latestSender(floatSetTextScale);
function setTextScale(value: number) {
  textScale = value;
  void scaleSending.send(value);
}

/** Ctrl and Alt together are AltGr on layouts such as ABNT2: they do nothing here. */
const altGr = (event: MouseEvent) => event.ctrlKey && event.altKey;

/** Ctrl+wheel sizes the text, Alt+wheel fades; so does the plain wheel over the bar. */
function wheel(event: WheelEvent) {
  if (event.deltaY === 0 || altGr(event)) return;
  const up = event.deltaY < 0;
  if (event.ctrlKey && !event.altKey) {
    event.preventDefault();
    setTextScale(stepTextScale(textScale, up));
    return;
  }
  const overBar = controls && (event.target as Element).closest(".bar") !== null;
  if ((event.altKey && !event.ctrlKey) || overBar) {
    event.preventDefault();
    setOpacity(stepOpacity(opacity, up));
  }
}

/** Over the title bar, resize strips or scrollbar, the body leaves the press alone. */
function theirs(event: MouseEvent) {
  const target = event.target as Element;
  if (collapsed || target.closest("button, input, .title-bar, .edge")) return true;
  const box = scroller.getBoundingClientRect();
  return event.clientX >= box.left + scroller.clientWidth && event.clientX < box.right;
}

/**
 * Dragging the sheet moves the window once the pointer has moved a little: the native move loop
 * would otherwise take the second click of a double click. Ctrl never moves it.
 */
const DRAG_START = 4;
let press: { pointer: number; x: number; y: number } | null = null;
function pointerdown(event: PointerEvent) {
  if (event.button !== 0 || theirs(event) || altGr(event)) return;
  event.preventDefault();
  press = event.ctrlKey ? null : { pointer: event.pointerId, x: event.clientX, y: event.clientY };
}
function pointermove(event: PointerEvent) {
  hovered = true;
  if (!press || event.pointerId !== press.pointer) return;
  // Released outside the window.
  if ((event.buttons & 1) === 0) {
    press = null;
    return;
  }
  if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < DRAG_START) return;
  press = null;
  void floatDrag().catch(() => {});
}
function pointerup() {
  press = null;
}

/** A double click edits the note in the panel; Ctrl+double-click returns the text to its size. */
function doubleClick(event: MouseEvent) {
  if (theirs(event) || altGr(event)) return;
  event.preventDefault();
  if (event.ctrlKey) setTextScale(1);
  else if (!gone) void floatEdit().catch(() => {});
}

onMount(() => {
  locale = session.state.locale;
  setLocale(session.state.locale);
  apply(session.state);
  session.listen(apply);
  // Links are text here: the panel opens them.
  const click = (event: MouseEvent) => {
    if ((event.target as Element).closest("a")) event.preventDefault();
  };
  body.addEventListener("click", click);
  // Not passive: Ctrl+wheel would otherwise zoom the page itself.
  sheet.parentElement?.addEventListener("wheel", wheel, { passive: false });
  return () => {
    body.removeEventListener("click", click);
    sheet.parentElement?.removeEventListener("wheel", wheel);
  };
});
</script>

<div
  class="float"
  class:active={controls}
  bind:this={windowRoot}
  role="group"
  aria-label={labels.window}
  onpointerenter={() => { hovered = true; }}
  onpointermove={pointermove}
  onpointerup={pointerup}
  onpointercancel={pointerup}
  onpointerleave={() => { hovered = false; }}
  onpointerdown={pointerdown}
  ondblclick={doubleClick}
>
  <div class="sheet" class:collapsed bind:this={sheet}>
    <div class="scroller" hidden={collapsed} bind:this={scroller}>
      {#if gone}<p class="gone" role="status">{labels.gone}</p>{/if}
      <!-- Built from the editor's parser as DOM nodes; note text never becomes HTML. -->
      <div class="body note-content" hidden={gone} bind:this={body} aria-label={name}></div>
    </div>
  </div>
  <TitleBar
    {name}
    shown={controls}
    {opacity}
    {clickThrough}
    {locale}
    {collapsed}
    {gone}
    onopacity={setOpacity}
    ontoggle={() => void floatSetCollapsed(!collapsed).catch(() => {})}
  />
  {#if !clickThrough || revealed}<FloatEdges horizontalOnly={collapsed} />{/if}
</div>

<style>
  :global(body:has(> .float)) {
    background: transparent;
  }

  /* `--pad` is the transparent gutter around the sheet, `SHADOW_PAD` in `floats.rs`. */
  .float {
    --pad: var(--space-2);
    /* Matches floats::TITLE_HEIGHT (28 logical px), including the top border. */
    --title-height: var(--size-control);
    position: fixed;
    inset: 0;
  }

  /* The scroller stays inset from both the title bar and the sheet's bottom edge. */
  .sheet {
    position: absolute;
    inset: var(--pad);
    overflow: hidden;
    padding: calc(var(--title-height) + var(--space-2) - 1px) 0 var(--space-2);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-fg);
    opacity: var(--sheet-opacity);
    transition: filter var(--duration);
  }

  .sheet.collapsed {
    padding: 0;
  }

  /* As a floating image's: the hairline and soft shadow show where the window ends. */
  .float.active .sheet {
    filter: drop-shadow(0 0 1px var(--color-float-ring))
      drop-shadow(0 1px var(--space-1) var(--color-float-shadow));
  }

  .scroller {
    height: 100%;
    overflow: auto;
    padding: 0 var(--space-3);
    user-select: none;
  }

  .body {
    font-size: var(--font-size-md);
    overflow-wrap: anywhere;
  }

  .body > :global(:first-child) {
    margin-top: 0;
  }

  .body > :global(:last-child) {
    margin-bottom: 0;
  }

  .body :global(.note-header) {
    padding-bottom: var(--space-2);
  }

  .body :global(a) {
    cursor: default;
  }

  /* Alone in the sheet, in its middle, clear of the controls. */
  .gone {
    display: grid;
    place-items: center;
    height: 100%;
    margin: 0;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    text-align: center;
  }

  @media (prefers-reduced-motion: reduce) {
    .sheet {
      transition: none;
    }
  }
</style>
