import { writable } from "svelte/store";
import { type NotePreviewMode, notesPreviewMode, notesSetPreviewMode } from "../../ipc";

export const notePreviewMode = writable<NotePreviewMode | null>(null);
let loaded: Promise<void> | undefined;

export function loadNotePreviewMode() {
  loaded ??= notesPreviewMode()
    .then((mode) => notePreviewMode.set(mode))
    .catch((error) => {
      loaded = undefined;
      throw error;
    });
  return loaded;
}

export async function setNotePreviewMode(mode: NotePreviewMode) {
  await notesSetPreviewMode(mode);
  notePreviewMode.set(mode);
}
