/**
 * Copy and cut in the panel write through Rust, which marks the text so Windows keeps it out of
 * clipboard history and cloud clipboard. The browser would write it unmarked. Every route (keys,
 * context menu) fires a copy or cut event, so the events are the only place to intercept. Plain
 * text only: the browser's HTML would also carry note image addresses that resolve nowhere else.
 */
import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

type Write = (text: string) => Promise<void>;

/** The panel shows a notice: the copy was cancelled, so the clipboard still holds the earlier item. */
const report = (error: unknown) => {
  console.error("[clipboard]", error);
  window.dispatchEvent(new CustomEvent("clipboard-failed"));
};

let queue: Promise<unknown> = Promise.resolve();
/**
 * Writes one at a time, in event order: a copy retrying on a busy clipboard must not land after a
 * later cut that already deleted its text.
 */
export function inOrder(write: Write): Write {
  return (text) => {
    const done = queue.then(() => write(text));
    queue = done.catch(() => {});
    return done;
  };
}

/** Installs the window-wide interception, the last listener a copy or cut event reaches. */
export function keepCopiesPrivate(unordered: Write) {
  const write = inOrder(unordered);
  const intercept = (event: ClipboardEvent) => void copy(event, write);
  window.addEventListener("copy", intercept);
  window.addEventListener("cut", intercept);
}

async function copy(event: ClipboardEvent, write: Write) {
  const data = event.clipboardData;
  if (event.defaultPrevented) {
    // A handler, such as the editor's copy, filled the event's data for the browser to write.
    // With no data, it wrote nothing, or wrote privately itself, as `privateCut` does.
    if (!data?.types.length) return;
    const text = data.types.includes("text/plain") ? data.getData("text/plain") : "";
    data.clearData();
    if (text) await write(text).catch(report);
    return;
  }
  // Writing nothing when there is no text fails closed: a selected image stays off the clipboard.
  event.preventDefault();
  const target = event.composedPath()[0];
  const field =
    target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement ? target : null;
  const start = field?.selectionStart ?? null;
  const end = field?.selectionEnd ?? null;
  if (!field || start === null || end === null) {
    // The browser cuts only in editable content, and the editor handles its own cut.
    if (event.type === "cut") return;
    // As the browser's copy, without non-breaking spaces.
    const text = document.getSelection()?.toString().replaceAll("\u00a0", " ");
    if (text) await write(text).catch(report);
    return;
  }
  const value = field.value;
  if (start === end) return;
  try {
    await write(value.slice(start, end));
  } catch (error) {
    report(error);
    return;
  }
  if (event.type !== "cut" || field.readOnly || field.disabled) return;
  const root = field.getRootNode() as Document | ShadowRoot;
  // Deletes only what was written: not if the user typed or moved meanwhile.
  if (
    root.activeElement === field &&
    field.value === value &&
    field.selectionStart === start &&
    field.selectionEnd === end
  )
    // Deprecated, but the only deletion that keeps the field's own undo.
    document.execCommand("delete");
}

/** Set while the editor runs its own handler on a synthetic event, which `privateCut` skips. */
let replaying = false;

/** Runs the editor's own copy or cut on a synthetic event and returns the text it would copy. */
function replay(view: EditorView, type: "copy" | "cut"): string | null {
  const data = new DataTransfer();
  replaying = true;
  try {
    view.contentDOM.dispatchEvent(
      new ClipboardEvent(type, { clipboardData: data, cancelable: true }),
    );
  } finally {
    replaying = false;
  }
  return data.types.includes("text/plain") ? data.getData("text/plain") : null;
}

/**
 * The editor's cut deletes the selection before the browser writes the clipboard; here the text is
 * written first and deleted only once written. The editor's own handlers produce both the text and
 * the deletion, so several cursors and whole-line cuts behave as in CodeMirror. Its copy needs no
 * change: `keepCopiesPrivate` takes the text it leaves in the event.
 */
export function privateCut(unordered: Write): Extension {
  const write = inOrder(unordered);
  return EditorView.domEventHandlers({
    cut(_event, view) {
      if (replaying) return false;
      const before = view.state;
      const text = replay(view, "copy");
      if (text === null) return false;
      write(text).then(() => {
        // Never delete something other than what was written.
        if (view.state.doc.eq(before.doc) && view.state.selection.eq(before.selection))
          replay(view, "cut");
      }, report);
      return true;
    },
  });
}
