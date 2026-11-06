import { get, writable } from "svelte/store";
import {
  type NoteOrder,
  notesGroupByDate,
  notesOrder,
  notesSetGroupByDate,
  notesSetOrder,
} from "../../ipc";
export const noteOrder = writable<NoteOrder>("modified-desc");
export const groupByDate = writable(false);
export const noteOrderBusy = writable(false);
let loaded: Promise<void> | undefined;
export function loadNoteOrder() {
  loaded ??= Promise.all([notesOrder(), notesGroupByDate()])
    .then(([value, grouped]) => {
      noteOrder.set(value);
      groupByDate.set(grouped);
    })
    .catch((error) => {
      loaded = undefined;
      throw error;
    });
  return loaded;
}
export async function setGroupByDate(enabled: boolean) {
  if (get(noteOrderBusy)) return;
  noteOrderBusy.set(true);
  try {
    await loadNoteOrder();
    await notesSetGroupByDate(enabled);
    groupByDate.set(enabled);
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
