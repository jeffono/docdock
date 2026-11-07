import { get, writable } from "svelte/store";
import {
  type Grouping,
  type NoteOrder,
  notesGrouping,
  notesOrder,
  notesSetGrouping,
  notesSetOrder,
} from "../../ipc";
export const noteOrder = writable<NoteOrder>("modified-desc");
export const grouping = writable<Grouping>("none");
export const noteOrderBusy = writable(false);
let loaded: Promise<void> | undefined;
export function loadNoteOrder() {
  loaded ??= Promise.all([notesOrder(), notesGrouping()])
    .then(([value, grouped]) => {
      noteOrder.set(value);
      grouping.set(grouped);
    })
    .catch((error) => {
      loaded = undefined;
      throw error;
    });
  return loaded;
}
export async function setGrouping(value: Grouping) {
  if (get(noteOrderBusy)) return;
  noteOrderBusy.set(true);
  try {
    await loadNoteOrder();
    await notesSetGrouping(value);
    grouping.set(value);
  } finally {
    noteOrderBusy.set(false);
  }
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
