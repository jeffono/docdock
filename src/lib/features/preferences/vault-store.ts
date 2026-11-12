import { writable } from "svelte/store";
import { vaultFile } from "../../ipc";

/** Shared by Preferences and the unavailable list, which can stay mounted behind it. */
export const vaultPath = writable<string | null>(null);
export async function loadVault() {
  vaultPath.set(await vaultFile());
}
