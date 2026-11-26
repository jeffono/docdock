import type { NotesFolder } from "../../ipc";
import { type FolderAction, type FolderGone, folderAction } from "../notes/folder-state";

export type ScratchpadSyncAction = "switch" | FolderAction;

/**
 * Decides how Scratchpad reacts when the notes folder is re-read (watcher, trust, or folder
 * picker). A different path switches cleanly to the new folder; the same path follows
 * `folderAction` using the effective shown state (`folderGone` if a write failed before the
 * watcher updated `folder.state`), preserving unsaved edits and editor undo history.
 */
export function scratchpadSyncAction(
  current: NotesFolder | null,
  folderGone: FolderGone | null,
  next: NotesFolder | null,
  hasEditor: boolean,
  dirty: boolean,
): ScratchpadSyncAction {
  if ((current?.path ?? null) !== (next?.path ?? null)) return "switch";
  const shown = folderGone ?? current?.state ?? null;
  return folderAction(shown, next?.state ?? null, hasEditor, dirty);
}
