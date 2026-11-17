import handleSource from "../../../../src-tauri/src/handle.rs?raw";
import type { TabSize } from "../../ipc";
import edgeTabSource from "../tab/EdgeTab.svelte?raw";
import { unifiedDiff } from "./diff";

/** The tab's size, which Rust shapes the window to, and the wordmark's length, the UI's own. */
export type TabTuning = TabSize & { wordmark: number };

/** The size with its release constants in `handle.rs` and the ranges `dev_tab_shape` accepts. */
const sizeFields = [
  { key: "depth", constant: "DEPTH", label: "Depth (logical px)", range: [4, 64], step: 1 },
  { key: "height", constant: "HEIGHT", label: "Height (logical px)", range: [40, 400], step: 2 },
  {
    key: "curve",
    constant: "CURVE",
    label: "Curve height (logical px, at most half the height)",
    range: [0, 200],
    step: 1,
  },
] as const;

export const tabFields = [
  ...sizeFields,
  {
    key: "wordmark",
    constant: "WORDMARK",
    label: "Wordmark length (px)",
    range: [10, 300],
    step: 1,
  },
] as const;

const sizePattern = (constant: string) => new RegExp(`^const ${constant}: u32 = (\\d+);$`);
const wordmarkPattern = /^const WORDMARK = (\d+);$/;

function find(text: string, pattern: RegExp, what: string) {
  const match = text
    .split(/\r?\n/)
    .map((line) => pattern.exec(line))
    .find(Boolean);
  if (!match) throw new Error(`Cannot find ${what}`);
  return Number(match[1]);
}

/** The release values as `handle.rs` and `EdgeTab.svelte` spell them. */
export function parseTab(handle = handleSource, edgeTab = edgeTabSource): TabTuning {
  const size = Object.fromEntries(
    sizeFields.map(({ key, constant }) => [
      key,
      find(handle, sizePattern(constant), `${constant} in handle.rs`),
    ]),
  ) as TabSize;
  return { ...size, wordmark: find(edgeTab, wordmarkPattern, "WORDMARK in EdgeTab.svelte") };
}

/** Within each range, the two curves fitting in the height, as `dev_tab_shape` checks. */
export function validTab(value: unknown): value is TabTuning {
  if (!value || typeof value !== "object") return false;
  const fields = value as Record<string, unknown>;
  const inRange = tabFields.every(({ key, range: [min, max] }) => {
    const field = fields[key];
    return Number.isInteger(field) && (field as number) >= min && (field as number) <= max;
  });
  return inRange && (fields.curve as number) * 2 <= (fields.height as number);
}

export const tabDefaults = parseTab();

/** `handle.rs` lines with the size constants set to `size`; `exportHandle` diffs them. */
export function editSize(lines: string[], size: TabSize) {
  return lines.map((line) => {
    const field = sizeFields.find(({ constant }) => sizePattern(constant).test(line));
    return field ? `const ${field.constant}: u32 = ${size[field.key]};` : line;
  });
}

/** The `EdgeTab.svelte` constant to change for `wordmark`, as a unified diff. */
export function exportWordmark(wordmark: number, text = edgeTabSource) {
  const lines = text.split(/\r?\n/);
  const edited = lines.map((line) =>
    wordmarkPattern.test(line) ? `const WORDMARK = ${wordmark};` : line,
  );
  return (
    unifiedDiff("src/lib/features/tab/EdgeTab.svelte", lines, edited) ?? "No wordmark changes."
  );
}
