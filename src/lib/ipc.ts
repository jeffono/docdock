// The only point of contact between the UI and the Rust backend.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export function appVersion(): Promise<string> {
  return invoke<string>("app_version");
}

export type Locale = "en" | "pt-BR";
export const appLocale = () => invoke<Locale>("app_locale");

export function panelActivate(): Promise<void> {
  return invoke("panel_activate");
}

export function panelSetPinned(pinned: boolean): Promise<void> {
  return invoke("panel_set_pinned", { pinned });
}

export function panelSetEditing(editing: boolean): Promise<void> {
  return invoke("panel_set_editing", { editing });
}

export function webOpen(url: string): Promise<void> {
  return invoke("web_open", { url });
}

export function webClose(): Promise<void> {
  return invoke("web_close");
}

export type NotesFolder = { path: string; available: boolean };
export type Note = { path: string; name: string };
export const notesFolder = () => invoke<NotesFolder | null>("notes_folder");
export const notesChoose = () => invoke<NotesFolder | null>("notes_choose");
export const notesList = () => invoke<Note[]>("notes_list");
/** `version` is opaque: pass it back to `notesSave` as `base`. */
export type OpenNote = {
  html: string;
  text: string;
  version: string;
  title: string | null;
  tags: string[];
};
export type SaveResult = { status: "saved"; version: string } | { status: "conflict" };
export const notesOpen = (path: string) => invoke<OpenNote>("notes_open", { path });
export const notesSave = (path: string, content: string, base: string) =>
  invoke<SaveResult>("notes_save", { path, content, base });
export const notesPasteImage = (path: string, bytes: Uint8Array) =>
  invoke<string>("notes_paste_image", bytes, {
    headers: { "x-note-path": encodeURIComponent(path) },
  });

export const openExternal = (url: string) => invoke<void>("open_external", { url });

/** Something in the notes folder changed outside the app, or a save landed. */
export const onNotesChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen("notes-changed", handler);

/** A backup an interrupted save left next to `note`; `modified` is in ms since the Unix epoch. */
export type Leftover = { path: string; note: string; modified: number };
export const notesLeftovers = () => invoke<Leftover[]>("notes_leftovers");
/** Resolves to the path of the restored note. */
export const notesRestore = (path: string) => invoke<string>("notes_restore", { path });
