import type { EditorSelection, Text } from "@codemirror/state";
import { frontmatter, noteParser } from "./markdown";

const words = new Intl.Segmenter(undefined, { granularity: "word" });
const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });
const lineBreak = /^[\r\n\v\f\u0085\u2028\u2029]/u;

/** Words and characters of the text around images, whose Markdown is no prose; and the images. */
export type Counts = { words: number; chars: number; images: number };

export const readingRates = [300, 175] as const;
export const speakingRates = [160, 130] as const;

const pad = (value: number) => String(value).padStart(2, "0");

/** Seconds as `15s`, then a clock: `01:20`, or `1:05:00` from an hour. */
function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const clock = `${pad(Math.floor(seconds / 60) % 60)}:${pad(seconds % 60)}`;
  return seconds < 3600 ? clock : `${Math.floor(seconds / 3600)}:${clock}`;
}

export function formatTimeRange(words: number, fastWpm: number, slowWpm: number): string {
  if (words === 0) return "";
  const fast = formatDuration(Math.ceil((words * 60) / fastWpm));
  const slow = formatDuration(Math.ceil((words * 60) / slowWpm));
  return fast === slow ? fast : `${fast}~${slow}`;
}

/**
 * The text without its images' Markdown, and how many there were. Each becomes a line break, which
 * keeps the words on either side apart and counts as no character.
 */
function withoutImages(text: string): { prose: string; images: number } {
  // Cheap way out for the common note without any image syntax.
  if (!text.includes("![")) return { prose: text, images: 0 };
  let prose = "";
  let at = 0;
  let images = 0;
  noteParser.parse(text).iterate({
    enter(node) {
      if (node.name !== "Image") return;
      images++;
      prose += `${text.slice(at, node.from)}\n`;
      at = node.to;
      return false;
    },
  });
  return { prose: prose + text.slice(at), images };
}

export function countText(source: string): Counts {
  const { prose: text, images } = withoutImages(source);
  const count = { words: 0, chars: 0, images };
  for (const segment of words.segment(text)) {
    if (segment.isWordLike) count.words++;
  }
  for (const { segment } of graphemes.segment(text)) {
    if (!lineBreak.test(segment)) count.chars++;
  }
  return count;
}

export function countNote(text: string): Counts {
  const lines = text.split(/\r\n?|\n/);
  const matter = frontmatter((n) => lines[n - 1], lines.length);
  return countText(matter ? lines.slice(matter.end).join("\n") : text);
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
    count ??= { words: 0, chars: 0, images: 0 };
    if (range.to <= bodyStart) continue;
    const selected = countText(doc.sliceString(Math.max(range.from, bodyStart), range.to));
    count.words += selected.words;
    count.chars += selected.chars;
    count.images += selected.images;
  }
  return count;
}
