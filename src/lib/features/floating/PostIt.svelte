<script lang="ts">
// A post-it: one note shown read-only in a floating window, placed by Rust, which keeps it on top
// without ever activating it. It has no capability: it hears what to show through its IPC
// channel, reads its note through its own command whenever Rust says the note may have changed,
// and reaches only its own commands. The note sits on a sheet inset by a transparent gutter, where
// its hover shadow shows and the resize strips sit. Its controls show on hover, or while Alt held
// over a click-through window reveals them; the sheet fades, never the controls. A double click,
// or the pencil, edits the note in place (`edit-state.ts` decides, this file runs the effects);
// "Open in panel" edits it there.
import { onMount } from "svelte";
import { errorText, setLocale, t } from "../../i18n";
import {
  type FloatUpdate,
  floatBeginEdit,
  floatClipboardWrite,
  floatClose,
  floatDrag,
  floatEdit,
  floatEndEdit,
  floatFinished,
  floatNote,
  floatReady,
  floatSaveNote,
  floatSetCollapsed,
  floatSetOpacity,
  floatSetTextScale,
} from "../../ipc";
import Button from "../../ui/Button.svelte";
import Notice from "../../ui/Notice.svelte";
import { type Autosave, type AutosaveState, createAutosave } from "../notes/autosave";
import type MarkdownEditor from "../notes/MarkdownEditor.svelte";
import { renderNote } from "../notes/render";
import "../notes/content.css";
import { type EditEvent, type EditState, type Effect, reading, step } from "./edit-state";
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
let opacityOpen = $state(false);
/** The note is gone, as after a rename the post-it did not see: only that shows, until it returns. */
let gone = $state(false);
let edit = $state<EditState>(reading);
/** The editor shows: being edited, saving on the way out, in a conflict or parked (hidden). */
const editing = $derived(edit.mode !== "read" && edit.mode !== "entering");
// While editing the window is active: its controls and save state stay in sight.
const controls = $derived(revealed || (hovered && !clickThrough) || editing);
const labels = $derived.by(() => {
  void locale;
  return {
    window: t("postIt.label"),
    gone: t("postIt.gone"),
    busy: t("postIt.busy"),
    conflict: t("postIt.conflict"),
    reload: t("postIt.reload"),
    keepMine: t("postIt.keepMine"),
    parked: t("postIt.parked"),
    discard: t("postIt.discard"),
  };
});
const name = $derived(path?.slice(path.lastIndexOf("/") + 1).replace(/\.md$/i, "") ?? "");

/** The answer and the channel may arrive in either order: only a newer version counts. */
let latest = -1;
let revision = -1;
let escapeRevision = 0;
let parked = false;
// Requests already seen when the window loaded belong to an editor it no longer has.
let finishSeen = 0;
let closeSeen = 0;
function apply(update: FloatUpdate) {
  if (update.version <= latest) return;
  latest = update.version;
  const moved = update.path !== path;
  path = update.path;
  if (moved) editorCore?.setNotePath(update.path);
  // While the user is still changing them, the controls keep the latest values they chose.
  if (!opacitySending.busy()) opacity = update.opacity;
  if (!scaleSending.busy()) textScale = update.textScale;
  if (
    update.escapeRevision !== escapeRevision ||
    update.clickThrough !== clickThrough ||
    (clickThrough && !update.revealed) ||
    (update.collapsed && !collapsed)
  ) {
    opacityOpen = false;
  }
  escapeRevision = update.escapeRevision;
  // A window that starts or stops letting clicks through waits for the pointer to come back.
  if (update.clickThrough !== clickThrough) hovered = false;
  clickThrough = update.clickThrough;
  revealed = update.revealed;
  collapsed = update.collapsed;
  if (update.parked !== parked) {
    parked = update.parked;
    dispatch(parked ? { type: "parked", dirty: dirty() } : { type: "unparked" });
  }
  if (update.revision !== revision || moved) {
    revision = update.revision;
    dispatch({ type: "disk", dirty: dirty() });
  }
  if (update.finish !== finishSeen) {
    finishSeen = update.finish;
    dispatch({ type: "finish", request: update.finish });
  }
  if (update.closeRequested !== closeSeen) {
    closeSeen = update.closeRequested;
    dispatch({ type: "leave", after: "close" });
  }
}

// The inline editor, loaded on the first edit.
let Editor = $state<typeof MarkdownEditor | null>(null);
let editorCore = $state<MarkdownEditor>();
let editorBox = $state<HTMLElement>();
/** The text the editor opened with; `null` while no editor exists. */
let opened = $state<string | null>(null);
let raw = "";
/** The note's version the saves are based on. */
let version = "";
let lease = 0;
/** "Keep mine" on a note that is gone writes it again. */
let recreate = false;
let saving: Autosave | null = null;
let status = $state<AutosaveState>("saved");
let saveError = $state("");
/** Bumps on each edit, so a read from disk that started before one is dropped. */
let edits = 0;
let busy = $state(false);
let busyTimer: ReturnType<typeof setTimeout> | undefined;
const statusText = $derived.by(() => {
  void locale;
  if (edit.mode !== "editing" && edit.mode !== "leaving") return "";
  if (status === "saved") return t("editor.saved");
  return status === "saving" || status === "pending" ? t("editor.saving") : "";
});

function dirty(): boolean {
  return (saving?.unsaved() ?? null) !== null;
}

function dispatch(event: EditEvent) {
  const next = step(edit, event);
  edit = next.state;
  for (const effect of next.effects) run(effect);
}

/** Rust's calls, in the order decided: "Open in panel" waits for the edit to end. */
let calls: Promise<unknown> = Promise.resolve();
function call(send: () => Promise<unknown>) {
  calls = calls.then(send).catch(() => {});
}

function run(effect: Effect) {
  switch (effect.type) {
    case "begin":
      void begin();
      break;
    case "refused":
      busy = true;
      clearTimeout(busyTimer);
      busyTimer = setTimeout(() => {
        busy = false;
      }, 3000);
      break;
    case "flush":
      void flush();
      break;
    case "unmount":
      unmount();
      break;
    case "end":
      call(() => floatEndEdit(effect.handOff ?? false));
      break;
    case "answer":
      call(() => floatFinished(effect.request, effect.saved));
      break;
    case "close":
      call(floatClose);
      break;
    case "panel":
      call(floatEdit);
      break;
    case "render":
      void read();
      break;
    case "reload":
      void reload(effect.force);
      break;
    case "keep":
      void keepMine();
      break;
    case "check":
      void check();
      break;
    case "resume":
      void resume();
      break;
  }
}

async function begin() {
  let granted: Awaited<ReturnType<typeof floatBeginEdit>>;
  try {
    Editor ??= (await import("../notes/MarkdownEditor.svelte")).default;
    granted = await floatBeginEdit();
  } catch {
    granted = null;
  }
  if (!granted) {
    dispatch({ type: "refused" });
    return;
  }
  busy = false;
  // The folder went while Rust granted it: the note's text never reaches the editor.
  if (parked) {
    dispatch({ type: "granted", parked: true });
    return;
  }
  lease = granted.lease;
  recreate = false;
  raw = granted.text;
  opened = granted.text;
  setupAutosave(granted.version);
  dispatch({ type: "granted", parked: false });
}

function setupAutosave(base: string) {
  saving?.stop();
  version = base;
  status = "saved";
  saveError = "";
  saving = createAutosave({
    version: base,
    save: async (content, from) => {
      // Rust refuses it too: the folder is gone.
      if (edit.mode === "parked") throw new Error("parked");
      const result = await floatSaveNote(content, from, lease, recreate);
      if (result.status === "saved") {
        version = result.version;
        recreate = false;
      }
      return result;
    },
    onState: (next, cause) => {
      status = next;
      saveError = cause && edit.mode !== "parked" ? errorText(cause) : "";
      if (next === "conflict") dispatch({ type: "conflict" });
    },
  });
}

async function flush() {
  const current = saving;
  const result = current ? await current.flush() : "saved";
  // A save the editor no longer runs, as one before a pause in a parked folder, reports nothing.
  if (current === saving) dispatch({ type: "flushed", result });
}

function unmount() {
  saving?.stop();
  saving = null;
  opened = null;
  status = "saved";
  saveError = "";
  shown = null;
  void read();
}

/**
 * The editor takes the note from disk: always after "Reload" in a conflict, otherwise only while
 * clean. Only if the editor is still the one asked for, in the same mode.
 */
async function reload(force: boolean) {
  const at = edits;
  const current = saving;
  const mode = force ? "conflict" : "editing";
  let note: Awaited<ReturnType<typeof floatNote>>;
  try {
    note = await floatNote();
  } catch (e) {
    // The conflict stays, to choose again.
    if (force && current === saving && edit.mode === mode) saveError = errorText(e);
    return;
  }
  if (!editorCore || edit.mode !== mode || current !== saving) return;
  if (!force && (at !== edits || dirty())) return;
  if (!note) {
    // Nothing left to reload: the edit ends, and the post-it says the note went.
    if (force) {
      edit = reading;
      unmount();
      call(floatEndEdit);
    }
    return;
  }
  setupAutosave(note.version);
  editorCore.reloadText(note.text);
  raw = note.text;
  if (force) dispatch({ type: "resolved" });
}

/**
 * As the Scratchpad's "Keep my version": the editor's text saved over the note on disk now. The
 * conflict ends once that save lands; otherwise it stays, to choose again.
 */
async function keepMine() {
  const current = saving;
  const stale = () => current !== saving || edit.mode !== "conflict";
  let note: Awaited<ReturnType<typeof floatNote>>;
  try {
    note = await floatNote();
  } catch (e) {
    if (!stale()) saveError = errorText(e);
    return;
  }
  // Discarded, parked, or another choice already under way meanwhile.
  if (stale()) return;
  recreate = note === null;
  setupAutosave(note?.version ?? version);
  const keeping = saving;
  keeping?.edit(raw);
  const result = keeping ? await keeping.flush() : "saved";
  if (keeping === saving && edit.mode === "conflict" && result === "saved") {
    dispatch({ type: "resolved" });
  }
}

async function check() {
  try {
    const note = await floatNote();
    dispatch({ type: "checked", same: note?.version === version });
  } catch {
    // A window closing, or its folder leaving, meanwhile.
  }
}

/** Saves what is not on disk on the version the saves are based on, after a pause or a conflict. */
async function resume() {
  if (!dirty()) return;
  setupAutosave(version);
  saving?.edit(raw);
  await flush();
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

function toggleOpacity() {
  opacityOpen = !opacityOpen;
  // A collapsed window has only the title bar's height.
  if (opacityOpen && collapsed) {
    void floatSetCollapsed(false).catch(() => {
      opacityOpen = false;
    });
  }
}

function dismissOpacity(event: PointerEvent) {
  if (opacityOpen && !(event.target as Element).closest(".opacity-control")) opacityOpen = false;
}

function pointerleave() {
  hovered = false;
  opacityOpen = false;
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
  const overBar = controls && (event.target as Element).closest(".bar, .collapse-toggle") !== null;
  if ((event.altKey && !event.ctrlKey) || overBar) {
    event.preventDefault();
    setOpacity(stepOpacity(opacity, up));
  }
}

/** Over the title bar, resize strips or scrollbar, the body leaves the press alone. */
function theirs(event: MouseEvent) {
  const target = event.target as Element;
  if (collapsed || target.closest("button, input, .title-bar, .edge, .edit")) return true;
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
  // While editing, only the title bar moves the window.
  if (event.button !== 0 || edit.mode !== "read" || theirs(event) || altGr(event)) return;
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

/** A double click edits the note here; Ctrl+double-click returns the text to its size. */
function doubleClick(event: MouseEvent) {
  if (edit.mode !== "read" || theirs(event) || altGr(event)) return;
  event.preventDefault();
  if (event.ctrlKey) setTextScale(1);
  else if (!gone) dispatch({ type: "begin" });
}

function keydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  if (opacityOpen) opacityOpen = false;
  // The editor leaves on its own Esc; this covers focus on a notice's button.
  else if (!event.defaultPrevented) dispatch({ type: "leave", after: "read" });
}

$effect(() => {
  if (editorBox) editorBox.style.zoom = String(textScale);
});

// An edit, and its notices, never hide behind the bar: the post-it expands, and stays so until
// reading again (the toggle is off meanwhile).
$effect(() => {
  if (collapsed && edit.mode !== "read") void floatSetCollapsed(false).catch(() => {});
});

onMount(() => {
  locale = session.state.locale;
  setLocale(session.state.locale);
  finishSeen = session.state.finish;
  closeSeen = session.state.closeRequested;
  // Reloaded while editing, as in development: the editor and its text are gone.
  if (session.state.editing) call(floatEndEdit);
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
    clearTimeout(busyTimer);
    saving?.stop();
    body.removeEventListener("click", click);
    sheet.parentElement?.removeEventListener("wheel", wheel);
  };
});
</script>

<svelte:window onkeydown={keydown} onblur={() => dispatch({ type: "blur" })} />

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
  onpointerleave={pointerleave}
  onpointerdowncapture={dismissOpacity}
  onpointerdown={pointerdown}
  ondblclick={doubleClick}
>
  <div class="sheet" class:collapsed bind:this={sheet}>
    <div class="scroller" hidden={collapsed || editing} bind:this={scroller}>
      {#if busy}<div class="notices"><Notice icon="alert">{labels.busy}</Notice></div>{/if}
      {#if gone}<p class="gone" role="status">{labels.gone}</p>{/if}
      <!-- Built from the editor's parser as DOM nodes; note text never becomes HTML. -->
      <div class="body note-content" hidden={gone} bind:this={body} aria-label={name}></div>
    </div>
    {#if Editor && opened !== null && editing}
      <div class="edit" hidden={collapsed}>
        {#if edit.mode === "parked"}
          <div class="notices">
            <Notice icon="alert">
              {labels.parked}
              {#snippet actions()}
                <Button variant="neutral" onclick={() => dispatch({ type: "discard" })}>{labels.discard}</Button>
              {/snippet}
            </Notice>
          </div>
        {:else if edit.mode === "conflict"}
          <div class="notices">
            <Notice icon="alert">
              {labels.conflict}
              {#snippet actions()}
                <Button variant="neutral" onclick={() => dispatch({ type: "resolve", keep: false })}>{labels.reload}</Button>
                <Button onclick={() => dispatch({ type: "resolve", keep: true })}>{labels.keepMine}</Button>
              {/snippet}
            </Notice>
            {#if saveError}<Notice icon="alert">{saveError}</Notice>{/if}
          </div>
        {:else if saveError}
          <div class="notices"><Notice icon="alert">{saveError}</Notice></div>
        {/if}
        <!-- Parked: hidden at once, its text kept until the folder returns or the user discards. -->
        <div class="editor" hidden={edit.mode === "parked"} bind:this={editorBox}>
          <Editor
            bind:this={editorCore}
            initialText={opened}
            label={name}
            {path}
            fill
            spelling={false}
            stats={false}
            copy={floatClipboardWrite}
            onleave={() => dispatch({ type: "leave", after: "read" })}
            onblur={() => queueMicrotask(() => dispatch({ type: "blur" }))}
            onchange={(next) => {
              raw = next;
              edits++;
              saving?.edit(next);
            }}
          />
        </div>
      </div>
    {/if}
  </div>
  <TitleBar
    {name}
    shown={controls}
    {opacity}
    {clickThrough}
    {locale}
    {collapsed}
    gone={gone || edit.mode === "parked"}
    editing={edit.mode !== "read"}
    status={statusText}
    onopacity={setOpacity}
    opacityOpen={opacityOpen && !collapsed}
    ontoggleopacity={toggleOpacity}
    canCollapse={edit.mode === "read"}
    ontoggle={() => {
      if (edit.mode === "read") void floatSetCollapsed(!collapsed).catch(() => {});
    }}
    onedit={() => dispatch({ type: "begin" })}
    onpanel={() => dispatch({ type: "leave", after: "panel" })}
    onclose={() => dispatch({ type: "leave", after: "close" })}
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

  .scroller[hidden],
  .edit[hidden],
  .editor[hidden] {
    display: none;
  }

  /* The panel's notice, inset as the text is. */
  .notices {
    --panel-inset-left: 0;
    --panel-inset-right: 0;
  }

  .notices :global(.notice) {
    flex-wrap: wrap;
  }

  .edit {
    --panel-inset-left: var(--space-2);
    --panel-inset-right: var(--space-2);
    --editor-pad-top: 0;
    --editor-pad-right: var(--space-3);
    --editor-pad-bottom: 0;
    --editor-pad-left: var(--space-3);
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .edit .notices {
    --panel-inset-left: var(--space-2);
    --panel-inset-right: var(--space-2);
  }

  .editor {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
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
