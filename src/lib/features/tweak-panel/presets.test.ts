import { expect, it } from "vitest";
import { presetKey, updatePresets } from "./presets";

function store() {
  let saved = "";
  return {
    setItem(key: string, value: string) {
      expect(key).toBe(presetKey);
      saved = value;
    },
    read: () => JSON.parse(saved),
  };
}
it("renames without changing preset contents", () => {
  const storage = store();
  const presets = { A: { tokens: { color: "white" } } };
  const next = updatePresets(storage, presets, { kind: "rename", name: "A", next: "B" });
  expect(next).toEqual({ B: presets.A });
  expect(storage.read()).toEqual(next);
  expect(presets).toHaveProperty("A");
});
it("refuses a rename collision without losing either preset", () => {
  expect(() =>
    updatePresets(store(), { A: 1, B: 2 }, { kind: "rename", name: "A", next: "B" }),
  ).toThrow();
});
it("requires explicit overwrite to replace saved contents", () => {
  const storage = store();
  expect(() => updatePresets(storage, { A: 1 }, { kind: "save", name: "A", value: 2 })).toThrow();
  expect(
    updatePresets(storage, { A: 1 }, { kind: "save", name: "A", value: 2, overwrite: true }),
  ).toEqual({ A: 2 });
  expect(storage.read()).toEqual({ A: 2 });
});
it("deletes only the selected preset in storage", () => {
  const storage = store();
  expect(updatePresets(storage, { A: 1, B: 2 }, { kind: "delete", name: "A" })).toEqual({ B: 2 });
  expect(storage.read()).toEqual({ B: 2 });
});
it("keeps the original map when persistence fails", () => {
  const presets = { A: 1 };
  expect(() =>
    updatePresets(
      {
        setItem() {
          throw new Error("Full");
        },
      },
      presets,
      { kind: "delete", name: "A" },
    ),
  ).toThrow();
  expect(presets).toEqual({ A: 1 });
});
it("round trips names that match object prototype properties", () => {
  const storage = store();
  const next = updatePresets(storage, {}, { kind: "save", name: "__proto__", value: 42 });
  expect(Object.hasOwn(next, "__proto__")).toBe(true);
  expect(Object.getOwnPropertyDescriptor(storage.read(), "__proto__")?.value).toBe(42);
});
