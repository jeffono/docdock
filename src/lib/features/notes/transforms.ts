import { isolateHistory } from "@codemirror/commands";
import { EditorSelection, type SelectionRange } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { MessageKey } from "../../i18n";
import {
  capitalizeWords,
  cycleCase,
  deduplicateLines,
  normalizeWhitespace,
  pureTransforms,
  sentenceCase,
  sortLines,
  type TransformGroup,
  type TransformId,
} from "../../text-transforms";
import type { CommandEdit } from "./command";
import { noteBodyStart } from "./count";
import { frontmatterEnabled } from "./live-formatting";

export { capitalizeWords, cycleCase, deduplicateLines, sentenceCase, sortLines };

type TransformContext = { partialStart: boolean; partialEnd: boolean };
type Transform = (text: string, locale: string, context?: TransformContext) => string;

function normalizeWhitespaceRun(text: string, _locale: string, context?: TransformContext): string {
  if (!context || (!context.partialStart && !context.partialEnd) || /[\r\n]/.test(text)) {
    return normalizeWhitespace(text);
  }
  let out = text.replace(/[ \t\f\v]+/g, " ");
  if (!context.partialStart) out = out.replace(/^ /, "");
  if (!context.partialEnd) out = out.replace(/ $/, "");
  return out;
}

interface Span {
  from: number;
  to: number;
}

type TransformRun = (view: EditorView, edit?: CommandEdit, scope?: "line-or-block") => boolean;

function transformSelection(
  transform: Transform,
  wholeLines = false,
  blockOnSlash = wholeLines,
): TransformRun {
  return (view, edit = view, scope) => {
    const { state } = edit;
    if (state.readOnly) return false;
    const locale = document.documentElement.lang || "en";
    const words = new Intl.Segmenter(locale, { granularity: "word" });
    const target = ({ from, to, empty }: SelectionRange): Span | null => {
      if (empty && scope === "line-or-block") {
        const { doc } = state;
        const line = doc.lineAt(from);
        if (!blockOnSlash) return { from: line.from, to: line.to };
        if (!line.text.trim()) return null;
        const firstBodyLine = state.facet(frontmatterEnabled)
          ? doc.lineAt(noteBodyStart(doc)).number
          : 1;
        let first = line.number;
        let last = line.number;
        while (first > firstBodyLine && doc.line(first - 1).text.trim()) first--;
        while (last < doc.lines && doc.line(last + 1).text.trim()) last++;
        return { from: doc.line(first).from, to: doc.line(last).to };
      }
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
        const partialStart = from > state.doc.lineAt(from).from;
        const partialEnd = to < state.doc.lineAt(to).to;
        const after = transform(before, locale, { partialStart, partialEnd });
        if (before === after) return [];
        const beforeLines = before.split("\n");
        const afterLines = after.split("\n");
        if (afterLines.length > beforeLines.length) {
          return [{ from, to, insert: after }];
        }
        // Keep surviving separators at their original positions, even in mixed-ending notes.
        let start = from;
        const changes = before.split("\n", afterLines.length).flatMap((line, index) => {
          const change = { from: start, to: start + line.length, insert: afterLines[index] };
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
    if (edit === view && changes.empty && selection.eq(state.selection)) return true;
    edit.dispatch({
      changes,
      selection,
      annotations: isolateHistory.of("full"),
      userEvent: "input",
    });
    return true;
  };
}

export const transformLabels: Record<TransformId, MessageKey> = {
  uppercase: "transform.upper",
  lowercase: "transform.lower",
  "sentence-case": "transform.sentence",
  "capitalize-words": "transform.capitalize",
  "digits-only": "transform.digitsOnly",
  alphanumeric: "transform.alphanumeric",
  "normalize-whitespace": "transform.normalizeWhitespace",
  "strip-accents": "transform.stripAccents",
  "remove-empty-lines": "transform.removeEmptyLines",
  "join-lines": "transform.joinLines",
  "sort-ascending": "transform.sortAscending",
  "sort-descending": "transform.sortDescending",
  "deduplicate-lines": "transform.deduplicate",
  "clean-url": "transform.cleanUrl",
  "url-decode": "transform.urlDecode",
  "format-json": "transform.formatJson",
  "minify-json": "transform.minifyJson",
};

const slashBlocks = new Set<TransformId>(["normalize-whitespace", "format-json", "minify-json"]);

type TransformCommand = {
  id: TransformId;
  label: MessageKey;
  group: TransformGroup;
  run: TransformRun;
};

export const textTransforms: readonly TransformCommand[] = pureTransforms.map(
  ({ id, group, run }) => ({
    id,
    label: transformLabels[id],
    group,
    run: transformSelection(
      id === "normalize-whitespace" ? normalizeWhitespaceRun : run,
      group === "lines",
      group === "lines" || slashBlocks.has(id),
    ),
  }),
);

export const cycleCaseCommand = transformSelection(cycleCase);
