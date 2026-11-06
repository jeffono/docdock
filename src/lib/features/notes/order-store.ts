import { get, writable } from "svelte/store";
import { type NoteOrder, notesOrder, notesSetOrder } from "../../ipc";
export const noteOrder = writable<NoteOrder>("modified-desc");
export const noteOrderBusy = writable(false);
let loaded: Promise<void> | undefined;
export function loadNoteOrder() {
  loaded ??= notesOrder()
    .then((value) => {
      noteOrder.set(value);
    })
    .catch((error) => {
      loaded = undefined;
      throw error;
    });
  return loaded;
}
export async function setNoteOrder(value: NoteOrder) {
  if (get(noteOrderBusy)) return;
  noteOrderBusy.set(true);
  try {
    await loadNoteOrder();
    await notesSetOrder(value);
    noteOrder.set(value);
  } finally {
    noteOrderBusy.set(false);
  }
}
