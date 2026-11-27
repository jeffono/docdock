import type { ClipboardEntry } from "../../ipc";

const fold = (value: string) => value.normalize("NFD").replace(/\p{M}/gu, "").toLocaleLowerCase();

export function filterClipboardEntries(
  entries: readonly ClipboardEntry[],
  query: string,
): ClipboardEntry[] {
  const needle = fold(query.trim());
  if (!needle) return [...entries];
  return entries.filter((entry) => fold(entry.text).includes(needle));
}

export function previewSnippet(text: string, maxChars = 240): string {
  return text
    .slice(0, maxChars * 2)
    .replace(/\s+/g, " ")
    .trim();
}
