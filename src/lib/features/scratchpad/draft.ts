export type DraftAppendEdit = {
  from: number;
  to: number;
  insert: string;
  nextRaw: string;
};

/** Computes a minimal end-of-document edit for appending `addition` separated by one blank line. */
export function appendDraftEdit(existing: string, addition: string): DraftAppendEdit | null {
  if (!addition) return null;
  const docText = existing.replace(/\r\n?|\n/g, "\n");
  const docAddition = addition.replace(/\r\n?|\n/g, "\n");
  const eol = existing.includes("\r\n") ? "\r\n" : "\n";
  const rawAddition = eol === "\r\n" ? docAddition.replace(/\n/g, "\r\n") : addition;
  if (!existing.trim()) {
    return existing === rawAddition
      ? null
      : { from: 0, to: docText.length, insert: docAddition, nextRaw: rawAddition };
  }
  const trailingMatch = existing.match(/(?:\r\n|\r|\n)+$/u);
  const trailingRawLen = trailingMatch ? trailingMatch[0].length : 0;
  const docTrailingMatch = docText.match(/\n+$/u);
  const docTrailingLen = docTrailingMatch ? docTrailingMatch[0].length : 0;
  const head = existing.slice(0, existing.length - trailingRawLen);
  return {
    from: docText.length - docTrailingLen,
    to: docText.length,
    insert: `\n\n${docAddition}`,
    nextRaw: `${head}${eol}${eol}${rawAddition}`,
  };
}

/** Appends `addition` to an existing Scratchpad draft separated by one blank line. */
export function appendDraftText(existing: string, addition: string): string {
  return appendDraftEdit(existing, addition)?.nextRaw ?? existing;
}
