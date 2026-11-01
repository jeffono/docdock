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
