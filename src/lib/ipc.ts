// Único ponto de contato entre a UI e o backend Rust.
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
export const notesOpen = (path: string) => invoke<string>("notes_open", { path });

export const openExternal = (url: string) => invoke<void>("open_external", { url });
