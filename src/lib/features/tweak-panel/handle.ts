import source from "../../../../src-tauri/src/handle.rs?raw";
import type { HandleTuning, TabSize } from "../../ipc";
import { unifiedDiff } from "./diff";
import { editSize } from "./tab-shape";

export type { HandleTuning } from "../../ipc";

/** Each tuned value with its release constant in `handle.rs` and the range `dev_handle` accepts. */
export const handleFields = [
  {
    key: "activationPercent",
    constant: "ACTIVATION_PERCENT",
    label: "Activation height (% of the tab)",
    range: [100, 1000],
    step: 10,
  },
  {
    key: "revealPercent",
    constant: "REVEAL_PERCENT",
    label: "Reveal radius (% of the tab)",
    range: [10, 1000],
    step: 10,
  },
  {
    key: "rushSpeed",
    constant: "RUSH_SPEED",
    label: "Rush speed (logical px/s)",
    range: [500, 20000],
    step: 100,
  },
] as const;

export function validHandle(value: unknown): value is HandleTuning {
  if (!value || typeof value !== "object") return false;
  return handleFields.every(({ key, range: [min, max] }) => {
    const field = (value as Record<string, unknown>)[key];
    return Number.isInteger(field) && (field as number) >= min && (field as number) <= max;
  });
}

const pattern = (constant: string) => new RegExp(`^pub const ${constant}: u32 = (\\d+);$`);

/** The release values as `handle.rs` spells them. */
export function parseHandle(text = source): HandleTuning {
  const lines = text.split(/\r?\n/);
  const entries = handleFields.map(({ key, constant }) => {
    const match = lines.map((line) => pattern(constant).exec(line)).find(Boolean);
    if (!match) throw new Error(`Cannot find ${constant} in handle.rs`);
    return [key, Number(match[1])];
  });
  return Object.fromEntries(entries) as HandleTuning;
}

export const handleDefaults = parseHandle();

/**
 * The `handle.rs` constants to change for `values` and the tab's `size`, as one unified diff: the
 * constants sit close enough for their hunks to share context.
 */
export function exportHandle(values: HandleTuning, size: TabSize, text = source) {
  const lines = text.split(/\r?\n/);
  const edited = editSize(
    lines.map((line) => {
      const field = handleFields.find(({ constant }) => pattern(constant).test(line));
      return field ? `pub const ${field.constant}: u32 = ${values[field.key]};` : line;
    }),
    size,
  );
  return unifiedDiff("src-tauri/src/handle.rs", lines, edited) ?? "No edge handle changes.";
}
