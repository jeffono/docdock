// Único ponto de contato entre a UI e o backend Rust.
import { invoke } from "@tauri-apps/api/core";

export function appVersion(): Promise<string> {
  return invoke<string>("app_version");
}
