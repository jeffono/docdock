export const presetKey = "docdock-tweak-presets-v2";
export type Presets = Record<string, unknown>;
export type PresetAction =
  | { kind: "save"; name: string; value: unknown; overwrite?: boolean }
  | { kind: "rename"; name: string; next: string }
  | { kind: "delete"; name: string };

/** Build first, then persist: failed storage never changes the caller's state. */
export function updatePresets(
  storage: Pick<Storage, "setItem">,
  presets: Presets,
  action: PresetAction,
): Presets {
  const next = { ...presets };
  const name = action.name.trim();
  if (!name) throw new Error("Enter a preset name.");
  if (action.kind === "save") {
    if (Object.hasOwn(next, name) && !action.overwrite)
      throw new Error("A preset already has that name.");
    Object.defineProperty(next, name, {
      value: action.value,
      enumerable: true,
      configurable: true,
      writable: true,
    });
  } else {
    if (!Object.hasOwn(next, name)) throw new Error("Choose an existing preset.");
    if (action.kind === "rename") {
      const label = action.next.trim();
      if (!label) throw new Error("Enter a preset name.");
      if (label === name) return presets;
      if (Object.hasOwn(next, label)) throw new Error("A preset already has that name.");
      Object.defineProperty(next, label, {
        value: next[name],
        enumerable: true,
        configurable: true,
        writable: true,
      });
    }
    delete next[name];
  }
  storage.setItem(presetKey, JSON.stringify(next));
  return next;
}
