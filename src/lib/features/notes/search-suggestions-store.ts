import { writable } from "svelte/store";
import { noteSearchSetSuggestions, noteSearchSuggestionsEnabled } from "../../ipc";

export const noteSearchSuggestions = writable<boolean | null>(null);
let loaded: Promise<void> | undefined;

export function loadNoteSearchSuggestions() {
  loaded ??= noteSearchSuggestionsEnabled()
    .then((enabled) => noteSearchSuggestions.set(enabled))
    .catch((error) => {
      loaded = undefined;
      throw error;
    });
  return loaded;
}

export async function setNoteSearchSuggestions(enabled: boolean) {
  await noteSearchSetSuggestions(enabled);
  noteSearchSuggestions.set(enabled);
}
