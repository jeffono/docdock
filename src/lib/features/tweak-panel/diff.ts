/**
 * A unified diff of `path` from `lines` to `edited`, which has the same number of lines, with three
 * lines of context; `null` without changes.
 */
export function unifiedDiff(path: string, lines: string[], edited: string[]): string | null {
  const ranges: { start: number; end: number }[] = [];
  lines.forEach((line, index) => {
    if (line === edited[index]) return;
    const start = Math.max(0, index - 3);
    const end = Math.min(lines.length, index + 4);
    const previous = ranges.at(-1);
    if (previous && start <= previous.end) previous.end = end;
    else ranges.push({ start, end });
  });
  if (!ranges.length) return null;
  return [
    `--- a/${path}`,
    `+++ b/${path}`,
    ...ranges.flatMap(({ start, end }) => [
      `@@ -${start + 1},${end - start} +${start + 1},${end - start} @@`,
      ...lines
        .slice(start, end)
        .flatMap((line, i) =>
          line === edited[start + i] ? [` ${line}`] : [`-${line}`, `+${edited[start + i]}`],
        ),
    ]),
    "",
  ].join("\n");
}
