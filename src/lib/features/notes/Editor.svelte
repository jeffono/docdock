<script lang="ts">
import { undo } from "@codemirror/commands";
import { Compartment, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { onMount, untrack } from "svelte";
import { errorText, t } from "../../i18n";
import { notesOpen, notesPasteImage, notesSave, type OpenNote } from "../../ipc";
import Button from "../../ui/Button.svelte";
import { type AutosaveState, createAutosave } from "./autosave";
import contentCssUrl from "./content.css?url";
import { editorExtensions, editorState } from "./editor";
import cssUrl from "./editor.css?url";
import { notePath } from "./live-formatting";
import { diskReload, rawHistory, rawText } from "./reload";

let {
  path,
  note,
  onleave,
  ondiscard,
  onstatus,
  tags,
}: {
  path: string;
  note: OpenNote;
  onleave: () => void;
  /** Leaves without saving, after the user chose to drop edits that cannot be saved. */
  ondiscard: () => void;
  onstatus: (state: AutosaveState) => void;
  /** The folder's tags, offered while typing them in the frontmatter. */
  tags: () => string[];
} = $props();
let host: HTMLDivElement;
let view: EditorView | undefined;
let raw = untrack(() => note.text);
const editable = new Compartment();
const location = new Compartment();
let revision = 0;
export function snapshot() {
  return revision;
}
let status = $state<AutosaveState>("saved");
let error = $state("");
let resolving = $state(false);
let pasting: Promise<void> | undefined;
let disposed = false;
let deleted = $state(false);
let updated = $state(false);
let replacing = false;
/** The text being edited, saved or not. */
export function text() {
  return raw;
}
export function focus() {
  view?.focus();
}
export function moved(next: string) {
  path = next;
  view?.dispatch({ effects: location.reconfigure(notePath.of(next)) });
  deleted = false;
  revision++;
}
export function missing() {
  const pending = saving.unsaved();
  if (status === "conflict" || status === "error") {
    saving.stop();
    saving = autosave("");
    if (pending !== null) saving.edit(pending);
  }
  deleted = true;
  updated = false;
  error = "";
  revision++;
  if (saving.unsaved() !== null) void saving.flush();
}
export function unavailable(cause: unknown) {
  error = errorText(cause);
  revision++;
}

function paste(event: ClipboardEvent, editor: EditorView): boolean {
  const data = event.clipboardData;
  // Let CodeMirror handle text, including clipboard data that also carries an image.
  if (!data || data.types.includes("text/plain")) return false;
  const file = Array.from(data.items)
    .find((item) => item.kind === "file")
    ?.getAsFile();
  if (!file) return false;
  event.preventDefault();
  if (pasting || resolving || editor.state.readOnly) return true;
  const selection = editor.state.selection;
  revision++;
  error = "";
  editor.dispatch({ effects: editable.reconfigure(EditorState.readOnly.of(true)) });
  pasting = (async () => {
    try {
      const attachment = await notesPasteImage(path, new Uint8Array(await file.arrayBuffer()));
      if (disposed) return;
      editor.dispatch({ selection });
      editor.dispatch(editor.state.replaceSelection(`![](${attachment})`));
    } catch (e) {
      if (!disposed) error = errorText(e);
    } finally {
      pasting = undefined;
      if (!disposed) {
        revision++;
        editor.dispatch({ effects: editable.reconfigure(EditorState.readOnly.of(resolving)) });
      }
    }
  })();
  return true;
}
function autosave(version: string) {
  status = "saved";
  onstatus(status);
  error = "";
  return createAutosave({
    version,
    save: async (text, base) => {
      // A rename in progress holds saves, which then go to the new name.
      while (holding) await holding;
      const result = await notesSave(path, text, deleted ? "" : base);
      if (result.status === "saved") deleted = false;
      return result;
    },
    onState: (next, cause) => {
      revision++;
      status = next;
      onstatus(next);
      error = cause ? errorText(cause) : "";
    },
  });
}
let saving = autosave(untrack(() => note.version));
const extensions = () => [
  rawText.init(() => raw),
  rawHistory,
  EditorView.updateListener.of((update) => {
    const next = update.state.field(rawText);
    if (next === raw || replacing) return;
    raw = next;
    updated = false;
    revision++;
    saving.edit(raw);
  }),
  editable.of(EditorState.readOnly.of(false)),
  location.of(notePath.of(path)),
  EditorView.domEventHandlers({ paste }),
  ...editorExtensions(
    onleave,
    (focused) => {
      // Deferred: moving the card in the DOM blurs the editor during a Svelte update, where changing
      // state throws and would leave the save half started.
      if (!focused) queueMicrotask(() => void saving.flush());
    },
    t("editor.label"),
    tags,
  ),
];

let holding: Promise<void> | null = null;
/**
 * Renames the note while it stays editable. `change` may rewrite the text first (a new frontmatter
 * title), saved as one undo step. With everything saved, later saves wait while `move` runs with the
 * version on disk, so none can bring the old name back; `move` calls `moved` on success. Typing goes
 * on meanwhile. `move` may still `rewrite` the text, saved once the move ends, as any edit then.
 * Returns false, having done nothing, when the note cannot be saved first.
 */
export async function rename(
  change: (text: string) => string | null,
  move: (
    version: string,
    changed: boolean,
    rewrite: (change: (text: string) => string | null) => boolean,
  ) => Promise<void>,
): Promise<boolean> {
  if (resolving || pasting || deleted || holding) return false;
  const next = change(raw);
  if (view && next !== null && next !== raw) view.dispatch(diskReload(view.state, next));
  if ((await saving.flush()) !== "saved" || holding) return false;
  let release = () => {};
  holding = new Promise((resolve) => {
    release = resolve;
  });
  try {
    const disk = await notesOpen(path);
    await move(disk.version, next !== null, (later) => {
      const text = later(raw);
      if (!view || text === null || text === raw) return false;
      view.dispatch(diskReload(view.state, text));
      return true;
    });
    return true;
  } finally {
    holding = null;
    release();
  }
}

export async function finish(change?: (text: string) => string): Promise<OpenNote | null> {
  if (resolving) return null;
  resolving = true;
  view?.dispatch({ effects: editable.reconfigure(EditorState.readOnly.of(true)) });
  // Blur while this Esc press is still being handled, as TextField does.
  view?.contentDOM.blur();
  try {
    await pasting;
    // Read after the await with editing locked: never replace text still being typed or pasted.
    // Only text edited here: a repair alone must not bring back a note deleted outside the app, or
    // overwrite one changed outside it that the watcher has not reloaded yet.
    const next = deleted || raw === note.text ? undefined : change?.(raw);
    if (view && next !== undefined && next !== raw) view.dispatch(diskReload(view.state, next));
    if ((await saving.flush()) !== "saved") return null;
    if (deleted) return { ...note, text: raw };
    return await notesOpen(path);
  } catch (e) {
    error = errorText(e);
    return null;
  } finally {
    resolving = false;
    view?.dispatch({ effects: editable.reconfigure(EditorState.readOnly.of(false)) });
  }
}

function replace(next: OpenNote) {
  revision++;
  saving.stop();
  saving = autosave(next.version);
  deleted = false;
  replacing = true;
  try {
    if (view && next.text !== raw) view.dispatch(diskReload(view.state, next.text));
  } finally {
    replacing = false;
  }
  raw = next.text;
}

/** Includes the text in flight. A watcher must never replace pending edits. */
export function reload(next: OpenNote, started: number | undefined): boolean {
  if (started !== revision || deleted || resolving || pasting || saving.unsaved() !== null)
    return false;
  const changed = next.text !== raw;
  replace(next);
  updated = changed;
  return true;
}

async function resolve(keep: boolean) {
  if (resolving || pasting) return;
  resolving = true;
  try {
    const disk = await notesOpen(path);
    if (keep) {
      // Read the latest editor text after the await; typing during the read is preserved too.
      saving.stop();
      deleted = false;
      saving = autosave(disk.version);
      saving.edit(raw);
      await saving.flush();
    } else replace(disk);
  } catch (e) {
    error = errorText(e);
  } finally {
    resolving = false;
  }
}

/** A save that keeps failing, such as for a note deleted outside the app, must not trap the user. */
function discardAndLeave() {
  saving.stop();
  ondiscard();
}

onMount(() => {
  const root = host.attachShadow({ mode: "open" });
  const css = document.createElement("link");
  css.rel = "stylesheet";
  css.href = cssUrl;
  const contentCss = document.createElement("link");
  contentCss.rel = "stylesheet";
  contentCss.href = contentCssUrl;
  root.append(contentCss, css);
  view = new EditorView({ root, parent: root, state: editorState(raw, extensions()) });
  view.focus();
  return () => {
    disposed = true;
    saving.stop();
    view?.destroy();
  };
});
</script>

<div class="editor">
  {#if deleted}
    <div class="notice" role="status">{t("editor.deleted")}</div>
  {:else if updated}
    <div class="notice" role="status">
      {t("editor.updated")}
      <button class="discard" onclick={() => { if (view) { undo(view); view.focus(); } }}>{t("editor.undo")}</button>
    </div>
  {/if}
  {#if status === "conflict"}
    <div class="notice" role="status">
      <p>{t("editor.conflict")}</p>
      <div class="actions">
        <Button onclick={() => resolve(true)} disabled={resolving}>{t("editor.keep")}</Button>
        <button class="discard" onclick={() => resolve(false)} disabled={resolving}>{t("editor.discard")}</button>
      </div>
    </div>
  {/if}
  {#if error}
    <div class="notice">
      <p class="error" role="alert">{error}</p>
      {#if status !== "conflict"}
        <div class="actions">
          <button class="discard" onclick={discardAndLeave}>{t("editor.discard")}</button>
        </div>
      {/if}
    </div>
  {/if}
  <div class="surface" bind:this={host}></div>
</div>

<style>
/* Grows with the text inside the card; the list is the only scroller. */
.notice { padding: var(--space-2) var(--space-3); border-bottom: 1px solid var(--color-line); font-size: var(--font-size-sm); }
p { margin: 0; }
.actions { display: flex; flex-wrap: wrap; gap: var(--space-2); margin-top: var(--space-2); }
.discard { border: 1px solid var(--color-line); border-radius: var(--radius-sm); background: var(--color-surface); color: var(--color-fg); font: inherit; padding: 0 var(--space-2); min-height: var(--size-control); }
.error { color: var(--color-danger); }
</style>
