import { get, type Writable, writable } from "svelte/store";
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
async function setPreference<T>(value: T, save: (value: T) => Promise<void>, store: Writable<T>) {
  if (get(noteOrderBusy)) return;
  noteOrderBusy.set(true);
  try {
    await loadNoteOrder();
    await save(value);
    store.set(value);
  } finally {
    noteOrderBusy.set(false);
  }
}
export const setRelativeDates = (enabled: boolean) =>
  setPreference(enabled, notesSetRelativeDates, relativeDates);
export const setGrouping = (value: Grouping) => setPreference(value, notesSetGrouping, grouping);
export const setNoteOrder = (value: NoteOrder) => setPreference(value, notesSetOrder, noteOrder);
