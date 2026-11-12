import { get, writable } from "svelte/store";
import {
  type Grouping,
  type NoteOrder,
  notesGrouping,
  notesOrder,
  notesRelativeDates,
  notesSetGrouping,
  notesSetOrder,
  notesSetRelativeDates,
} from "../../ipc";
export const noteOrder = writable<NoteOrder>("modified-desc");
export const relativeDates = writable(false);
export const grouping = writable<Grouping>("none");
export const noteOrderBusy = writable(false);
let loaded: Promise<void> | undefined;
export function loadNoteOrder() {
  loaded ??= Promise.all([notesOrder(), notesGrouping(), notesRelativeDates()])
    .then(([value, grouped, relative]) => {
      relativeDates.set(relative);
      noteOrder.set(value);
      grouping.set(grouped);
    })
    .catch((error) => {
      loaded = undefined;
      throw error;
    });
  return loaded;
}
export async function setRelativeDates(enabled: boolean) {
  if (get(noteOrderBusy)) return;
  noteOrderBusy.set(true);
  try {
    await loadNoteOrder();
    await notesSetRelativeDates(enabled);
    relativeDates.set(enabled);
  } finally {
    noteOrderBusy.set(false);
  }
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
