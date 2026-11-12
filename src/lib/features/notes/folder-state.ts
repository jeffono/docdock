import type { MessageKey } from "../../i18n";
import type { FolderState } from "../../ipc";

/** A state in which nothing is read from or written to the folder. */
export type FolderGone = Exclude<FolderState, "available">;

/** The list's explanation for a folder that is not available. */
export function folderNotice(state: FolderGone): MessageKey {
  return `notes.${state}`;
}

/**
 * What the list does when a reread finds the notes folder in a state:
 * - `reload`: still available; reread in place.
 * - `open`: back, or available for the first time, with no editor; start over as on choosing it.
 * - `resume`: back while editing; keep the editor, reread and save what it holds.
 * - `keep`: gone while the editor holds unsaved text; forget everything else, never that text.
 * - `close`: gone with nothing unsaved; forget everything, the editor included.
 */
export type FolderAction = "reload" | "open" | "resume" | "keep" | "close";

/** `shown` is the state the list shows; `null` is no folder chosen. */
export function folderAction(
  shown: FolderState | null,
  found: FolderState | null,
  editing: boolean,
  unsaved: boolean,
): FolderAction {
  if (found !== "available") return editing && unsaved ? "keep" : "close";
  if (shown === "available") return "reload";
  return editing ? "resume" : "open";
}
