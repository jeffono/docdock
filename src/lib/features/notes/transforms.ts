import { isolateHistory } from "@codemirror/commands";
import { EditorSelection, type SelectionRange } from "@codemirror/state";
import type { Command } from "@codemirror/view";
import type { MessageKey } from "../../i18n";

type Transform = (text: string, locale: string) => string;

const upper: Transform = (text, locale) => text.toLocaleUpperCase(locale);
const lower: Transform = (text, locale) => text.toLocaleLowerCase(locale);

function capitalize(text: string, locale: string) {
  return lower(text, locale).replace(/\p{L}/u, (letter) => upper(letter, locale));
}

export const sentenceCase: Transform = (text, locale) => {
  // Sentence segmentation treats a lowercase word after a period as a continuation.
  // Use uppercase for boundary detection, keeping the source's UTF-16 offsets.
  const boundaries = text.replace(/\p{Ll}/gu, (letter) => {
    const raised = upper(letter, locale);
    return raised.length === letter.length ? raised : letter;
  });
  const sentences = [
    ...new Intl.Segmenter(locale, { granularity: "sentence" }).segment(boundaries),
  ];
  return sentences
    .map(({ index, segment }) => capitalize(text.slice(index, index + segment.length), locale))
    .join("");
};

export const capitalizeWords: Transform = (text, locale) =>
  [...new Intl.Segmenter(locale, { granularity: "word" }).segment(text)]
    .map(({ segment, isWordLike }) => (isWordLike ? capitalize(segment, locale) : segment))
    .join("");

export const cycleCase: Transform = (text, locale) => {
  const lowercase = lower(text, locale);
  const sentence = sentenceCase(text, locale);
  const words = capitalizeWords(text, locale);
  const uppercase = upper(text, locale);
  const forms = [lowercase, sentence, words, uppercase];
  // Skip equivalent forms (for example, sentence case and word case on one word).
  const current = forms.lastIndexOf(text);
  return forms.slice(current + 1).find((form) => form !== text) ?? lowercase;
};

export function sortLines(text: string, locale: string, direction: 1 | -1): string {
  const { compare } = new Intl.Collator(locale, { numeric: true, sensitivity: "base" });
  return text
    .split("\n")
    .sort((a, b) => direction * compare(a, b))
    .join("\n");
}

export function deduplicateLines(text: string): string {
  return [...new Set(text.split("\n"))].join("\n");
}

interface Span {
  from: number;
  to: number;
}

function transformSelection(transform: Transform, wholeLines = false): Command {
  return (view) => {
    const { state } = view;
    if (state.readOnly) return false;
    const locale = document.documentElement.lang || "en";
    const words = new Intl.Segmenter(locale, { granularity: "word" });
    const target = ({ from, to, empty }: SelectionRange): Span | null => {
      if (wholeLines) {
        return empty
          ? null
          : { from: state.doc.lineAt(from).from, to: state.doc.lineAt(to - 1).to };
      }
      if (!empty) return { from, to };
      const line = state.doc.lineAt(from);
      const segments = words.segment(line.text);
      const at = from - line.from;
      const next = segments.containing(at);
      const word = next?.isWordLike ? next : segments.containing(at - 1);
      if (!word?.isWordLike) return null;
      return { from: line.from + word.index, to: line.from + word.index + word.segment.length };
    };

    // Expanded selections can share a line or a word. Transform that text only once.
    const spans: Span[] = [];
    const targets = state.selection.ranges.map(target);
    const ordered = targets
      .flatMap((span, index) => (span ? [{ span, index }] : []))
      .sort((a, b) => a.span.from - b.span.from);
    for (const { span, index } of ordered) {
      const previous = spans.at(-1);
      if (previous && span.from < previous.to) {
        previous.to = Math.max(previous.to, span.to);
        targets[index] = previous;
      } else {
        spans.push(span);
      }
    }
    if (!spans.length) return false;
    const changes = state.changes(
      spans.flatMap(({ from, to }) => {
        const before = state.sliceDoc(from, to);
        const after = transform(before, locale);
        if (before === after) return [];
        // Keep surviving separators at their original positions, even in mixed-ending notes.
        let start = from;
        const lines = after.split("\n");
        const changes = before.split("\n", lines.length).flatMap((line, index) => {
          const change = { from: start, to: start + line.length, insert: lines[index] };
          start = change.to + 1;
          return line === change.insert ? [] : [change];
        });
        if (start <= to) changes.push({ from: start - 1, to, insert: "" });
        return changes;
      }),
    );
    const ranges = state.selection.ranges.map((range, index) => {
      const span = targets[index];
      if (!span) return range.map(changes);
      const from = changes.mapPos(span.from, -1);
      const to = changes.mapPos(span.to, 1);
      return range.anchor > range.head
        ? EditorSelection.range(to, from)
        : EditorSelection.range(from, to);
    });
    const selection = EditorSelection.create(ranges, state.selection.mainIndex);
    if (changes.empty && selection.eq(state.selection)) return true;
    view.dispatch({
      changes,
      selection,
      annotations: isolateHistory.of("full"),
      userEvent: "input",
    });
    return true;
  };
}

type TransformCommand = {
  id: string;
  label: MessageKey;
  group: "case" | "lines";
  run: Command;
};

export const textTransforms: readonly TransformCommand[] = [
  { id: "uppercase", label: "transform.upper", group: "case", run: transformSelection(upper) },
  { id: "lowercase", label: "transform.lower", group: "case", run: transformSelection(lower) },
  {
    id: "sentence-case",
    label: "transform.sentence",
    group: "case",
    run: transformSelection(sentenceCase),
  },
  {
    id: "capitalize-words",
    label: "transform.capitalize",
    group: "case",
    run: transformSelection(capitalizeWords),
  },
  {
    id: "sort-ascending",
    label: "transform.sortAscending",
    group: "lines",
    run: transformSelection((text, locale) => sortLines(text, locale, 1), true),
  },
  {
    id: "sort-descending",
    label: "transform.sortDescending",
    group: "lines",
    run: transformSelection((text, locale) => sortLines(text, locale, -1), true),
  },
  {
    id: "deduplicate-lines",
    label: "transform.deduplicate",
    group: "lines",
    run: transformSelection(deduplicateLines, true),
  },
];

export const cycleCaseCommand = transformSelection(cycleCase);
