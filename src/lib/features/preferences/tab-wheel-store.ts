import { writable } from "svelte/store";
import { panelSetTabWheel, panelTabWheel, type TabWheelMode } from "../../ipc";

export const tabWheelMode = writable<TabWheelMode | null>(null);
let loaded: Promise<void> | undefined;

export function loadTabWheelMode() {
  loaded ??= panelTabWheel()
    .then((mode) => tabWheelMode.set(mode))
    .catch((error) => {
      loaded = undefined;
      throw error;
    });
  return loaded;
}

export async function setTabWheelMode(mode: TabWheelMode) {
  await panelSetTabWheel(mode);
  tabWheelMode.set(mode);
}
