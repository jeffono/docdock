import type { EditorSelection, Text } from "@codemirror/state";
import { frontmatter } from "./markdown";

const words = new Intl.Segmenter(undefined, { granularity: "word" });
const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });
const lineBreak = /^[\r\n\v\f\u0085\u2028\u2029]/u;

export type Counts = { words: number; chars: number };

export function countText(text: string): Counts {
  const count = { words: 0, chars: 0 };
  for (const segment of words.segment(text)) {
    if (segment.isWordLike) count.words++;
  }
  for (const { segment } of graphemes.segment(text)) {
    if (!lineBreak.test(segment)) count.chars++;
  }
  return count;
}

export function noteBodyStart(doc: Text): number {
  const matter = frontmatter((n) => doc.line(n).text, doc.lines);
  if (!matter) return 0;
  return matter.end < doc.lines ? doc.line(matter.end + 1).from : doc.length;
}

export function countSelection(doc: Text, selection: EditorSelection, bodyStart: number) {
  let count: Counts | null = null;
  for (const range of selection.ranges) {
    if (range.empty) continue;
    count ??= { words: 0, chars: 0 };
    if (range.to <= bodyStart) continue;
    const selected = countText(doc.sliceString(Math.max(range.from, bodyStart), range.to));
    count.words += selected.words;
    count.chars += selected.chars;
  }
  return count;
}
