import { writable } from "svelte/store";
import { spellingEnabled, spellingSetEnabled } from "../../ipc";

/** Off until the saved choice is read, so a disabled checker never flashes marks. */
export const spellingOn = writable(false);
let loaded: Promise<void> | undefined;
export function loadSpelling() {
  loaded ??= spellingEnabled()
    .then((enabled) => spellingOn.set(enabled))
    .catch((error) => {
      loaded = undefined;
      throw error;
    });
  return loaded;
}
export async function setSpelling(enabled: boolean) {
  await spellingSetEnabled(enabled);
  spellingOn.set(enabled);
}
