<script lang="ts">
import { EditorSelection } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { onMount, untrack } from "svelte";
import { errorText, type MessageKey, t } from "../../i18n";
import { notesOpen, notesPasteImage, notesSave, notesTrust, type OpenNote } from "../../ipc";
import Button from "../../ui/Button.svelte";
import UnlockVault from "../preferences/UnlockVault.svelte";
import { type AutosaveState, createAutosave } from "./autosave";
import type { FolderGone } from "./folder-state";
import { imageInsertion } from "./image-paste";
import { keepInPlace } from "./keep-in-place";
import MarkdownEditor from "./MarkdownEditor.svelte";
import type { EditStart } from "./reading-position";

let {
  path,
  note,
  onleave,
  ontrusted,
  ondiscard,
  onstatus,
  onlinkexternal,
  onlinkmenu,
  tags,
  start,
}: {
  path: string;
  note: OpenNote;
  onleave: () => void;
  /** Reclaims the edit after trusting the folder, before retrying the draft. */
  ontrusted: () => Promise<boolean>;
  /** Leaves without saving, after the user chose to drop edits that cannot be saved. */
  ondiscard: () => void;
  onstatus: (state: AutosaveState) => void;
  onlinkexternal: (url: URL) => void;
  onlinkmenu: (
    event: MouseEvent | KeyboardEvent,
    href: string,
    from: HTMLElement,
    at?: { x: number; y: number },
  ) => void;
  /** The folder's tags, offered while typing them in the frontmatter. */
  tags: () => string[];
  /** Where a double click on the reading view asked to start. */
  start?: EditStart;
} = $props();

let core = $state<MarkdownEditor>();
let raw = untrack(() => note.text);
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
/**
 * The notes folder left (a locked vault, an unplugged drive): the text stays only here, saving fails
 * without writing anywhere else, and `resume` saves it once the folder is back.
 */
let folderGone = $state<FolderGone | null>(null);
const goneText: Record<FolderGone, MessageKey> = {
  unavailable: "editor.folderUnavailable",
  missing: "editor.folderMissing",
  unrecognized: "editor.folderUnrecognized",
};
/** The errors that say the folder left, and the state each means. */
const goneCodes: Record<string, FolderGone> = {
  folderUnavailable: "unavailable",
  folderMissing: "missing",
  folderUnrecognized: "unrecognized",
};
/** Text changed in this editor, rather than only reloaded from disk. */
let typed = false;
/** The text being edited, saved or not. */
export function text() {
  return raw;
}
export function focus() {
  core?.focus();
}
export function moved(next: string) {
  path = next;
  core?.setNotePath(next);
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
  error = failure(cause);
  revision++;
}
/** Text the folder does not hold yet: an edit not saved, or an image being pasted. */
export function dirty() {
  return saving.unsaved() !== null || pasting !== undefined;
}
export function folderLeft(state: FolderGone) {
  folderGone = state;
  error = "";
  revision++;
}
/** The folder is back: save what it does not hold yet, against the version read, so a change made
 * meanwhile still conflicts. A save that started before it came back and fails is sent again. */
export function resume() {
  folderGone = null;
  saving.retry();
}
/** The user confirms the folder now at the path is theirs: the text waiting here saves into it. */
async function trustFolder() {
  try {
    await notesTrust();
    if (await ontrusted()) resume();
  } catch (cause) {
    error = errorText(cause);
  }
}
/** The text for an error, or nothing when the folder left, which its own notice explains. */
function failure(cause: unknown): string {
  const code = typeof cause === "object" && cause !== null && "code" in cause ? cause.code : null;
  const gone = Object.hasOwn(goneCodes, String(code)) ? goneCodes[String(code)] : undefined;
  if (gone) {
    folderLeft(gone);
    return "";
  }
  return errorText(cause);
}

function paste(event: ClipboardEvent, editor: EditorView): boolean {
  const data = event.clipboardData;
  // Let CodeMirror handle text, including clipboard data that also carries an image.
  if (!data || data.types.includes("text/plain")) return false;
  const file = Array.from(data.items)
    .find((item) => item.kind === "file")
    ?.getAsFile();
  if (!file) return false;
  insertImages(editor, [file]);
  return true;
}

function dragOver(event: DragEvent) {
  if (event.dataTransfer?.types.includes("Files")) event.preventDefault();
}

function drop(event: DragEvent) {
  const files = event.dataTransfer?.files;
  if (!files?.length) return;
  // CodeMirror would read dropped files as text.
  event.preventDefault();
  const images = Array.from(files).filter(
    (file) =>
      file.type.startsWith("image/") || (!file.type && /\.(gif|jpe?g|png|webp)$/i.test(file.name)),
  );
  const view = core?.getView();
  if (view && images.length) insertImages(view, images);
}

function insertImages(editor: EditorView, files: File[]) {
  if (pasting || resolving || holding || editor.state.readOnly) return;
  const selection = editor.state.selection;
  revision++;
  error = "";
  core?.setReadOnly(true);
  pasting = (async () => {
    try {
      const attachments: string[] = [];
      for (const file of files) {
        try {
          const bytes = new Uint8Array(await file.arrayBuffer());
          if (disposed) return;
          const attachment = await notesPasteImage(path, bytes);
          if (disposed) return;
          attachments.push(attachment);
        } catch (e) {
          if (disposed) return;
          error = failure(e);
          if (folderGone) break;
        }
      }
      if (attachments.length) {
        editor.dispatch({ selection });
        const { state } = editor;
        editor.dispatch(
          state.changeByRange(({ from, to }) => {
            const before = state.sliceDoc(state.doc.lineAt(from).from, from);
            const after = state.sliceDoc(to, state.doc.lineAt(to).to);
            const { insert, cursorOffset } = imageInsertion(before, after, attachments);
            return {
              changes: { from, to, insert },
              range: EditorSelection.cursor(from + cursorOffset),
            };
          }),
        );
      }
    } finally {
      pasting = undefined;
      if (!disposed) {
        revision++;
        core?.setReadOnly(resolving);
      }
    }
  })();
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
      if (next === "saved") folderGone = null;
      error = cause ? failure(cause) : "";
    },
  });
}
let saving = autosave(untrack(() => note.version));

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
  if (next !== null && next !== raw) core?.applyEdit(next);
  if ((await saving.flush()) !== "saved" || holding || pasting) return false;
  let release = () => {};
  holding = new Promise((resolve) => {
    release = resolve;
  });
  try {
    const disk = await notesOpen(path);
    await move(disk.version, next !== null, (later) => {
      const text = later(raw);
      if (!core?.getView() || text === null || text === raw) return false;
      core.applyEdit(text);
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
  core?.setReadOnly(true);
  // Blur while this Esc press is still being handled, as TextField does.
  core?.blur();
  try {
    await pasting;
    // Read after the await with editing locked: never replace text still being typed or pasted.
    // Only text edited here: a repair alone must not bring back a note deleted outside the app, or
    // overwrite one changed outside it that the watcher has not reloaded yet.
    // `note` follows the editor's own saves, so it cannot tell whether anything was typed.
    const next = deleted || !typed ? undefined : change?.(raw);
    if (next !== undefined && next !== raw) core?.applyEdit(next);
    if ((await saving.flush()) !== "saved") return null;
    if (deleted) return { ...note, text: raw };
    return await notesOpen(path);
  } catch (e) {
    error = failure(e);
    return null;
  } finally {
    resolving = false;
    core?.setReadOnly(false);
  }
}

function replace(next: OpenNote) {
  revision++;
  saving.stop();
  saving = autosave(next.version);
  deleted = false;
  core?.reloadText(next.text);
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
    error = failure(e);
  } finally {
    resolving = false;
  }
}

/** A save that keeps failing, such as for a note deleted outside the app, must not trap the user. */
function discardAndLeave() {
  saving.stop();
  ondiscard();
}

function prepareStart(view: EditorView, styled: Promise<unknown>) {
  const at = untrack(() => start);
  // Positions from the reading view apply only to the text it showed.
  if (at && at.text === raw && at.from <= view.state.doc.length) {
    const { state } = view;
    const range =
      at.word && state.sliceDoc(at.from, at.from + at.word.length) === at.word
        ? EditorSelection.range(at.from, at.from + at.word.length)
        : (state.wordAt(at.from) ?? EditorSelection.cursor(at.from));
    view.dispatch({ selection: range });
    // Laid out with its styles, the word goes back where it was clicked.
    void styled.then(() => {
      if (!disposed && view.dom.isConnected) keepInPlace(view, range.from, at.top);
    });
  }
  // A note created with only frontmatter (tags from the search) takes typing after it, not inside.
  else if (/^---\r?\n[\s\S]*\r?\n---\r?\n?$/.test(raw))
    view.dispatch({ selection: { anchor: view.state.doc.length } });
}

onMount(() => {
  return () => {
    disposed = true;
    saving.stop();
  };
});
</script>

<div class="editor">
  {#if deleted}
    <div class="notice" role="status">{t("editor.deleted")}</div>
  {:else if updated}
    <div class="notice" role="status">
      {t("editor.updated")}
      <button class="discard" onclick={() => core?.undo()}>{t("editor.undo")}</button>
    </div>
  {/if}
  {#if status === "conflict"}
    <div class="notice" role="status">
      <p>{t("editor.conflict")}</p>
      <div class="actions" hidden={folderGone !== null}>
        <Button onclick={() => resolve(true)} disabled={resolving}>{t("editor.keep")}</Button>
        <button class="discard" onclick={() => resolve(false)} disabled={resolving}>{t("editor.discard")}</button>
      </div>
    </div>
  {/if}
  {#if folderGone}
    <div class="notice" role="status">
      <p>{t(goneText[folderGone])}</p>
      <!-- Always offered, also after a conflict: leaving must never need the folder. -->
      <div class="actions">
        <UnlockVault />
        {#if folderGone === "unrecognized"}<Button onclick={trustFolder}>{t("notes.trust")}</Button>{/if}
        <button class="discard" onclick={discardAndLeave}>{t("editor.discard")}</button>
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
  <MarkdownEditor
    bind:this={core}
    initialText={note.text}
    label={t("editor.label")}
    {path}
    {tags}
    {onleave}
    onblur={() => queueMicrotask(() => void saving.flush())}
    onchange={(next) => {
      raw = next;
      typed = true;
      updated = false;
      revision++;
      saving.edit(raw);
    }}
    {onlinkexternal}
    {onlinkmenu}
    onpaste={paste}
    ondragover={dragOver}
    ondropcapture={drop}
    onspellerror={(e) => {
      error = errorText(e);
    }}
    onready={prepareStart}
  />
</div>

<style>
/* Grows with the text inside the card; the list is the only scroller. */
.notice { padding: var(--space-2) var(--space-3); border-bottom: 1px solid var(--color-line); font-size: var(--font-size-sm); }
p { margin: 0; }
.actions { display: flex; flex-wrap: wrap; gap: var(--space-2); margin-top: var(--space-2); }
.discard { border: 1px solid var(--color-line); border-radius: var(--radius-sm); background: var(--color-surface); color: var(--color-fg); font: inherit; padding: 0 var(--space-2); min-height: var(--size-control); }
.error { color: var(--color-danger); }
</style>
