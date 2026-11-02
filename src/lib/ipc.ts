// The only point of contact between the UI and the Rust backend.
import { invoke } from "@tauri-apps/api/core";

export function appVersion(): Promise<string> {
  return invoke<string>("app_version");
}

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
export type OpenNote = { html: string; text: string; version: string };
export type SaveResult = { status: "saved"; version: string } | { status: "conflict" };
export const notesOpen = (path: string) => invoke<OpenNote>("notes_open", { path });
export const notesSave = (path: string, content: string, base: string) =>
  invoke<SaveResult>("notes_save", { path, content, base });

export const openExternal = (url: string) => invoke<void>("open_external", { url });
