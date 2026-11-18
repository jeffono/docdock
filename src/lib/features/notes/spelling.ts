import { syntaxTree } from "@codemirror/language";
import { type EditorState, StateEffect } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  type EditorView,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";
import { frontmatter } from "./markdown";

/** Text that is not prose: code, addresses, markup and reference labels. */
const notProse = new Set([
  "FencedCode",
  "CodeBlock",
  "InlineCode",
  "URL",
  "Autolink",
  "HTMLBlock",
  "HTMLTag",
  "CommentBlock",
  "Comment",
  "ProcessingInstructionBlock",
  "LinkLabel",
  "FootnoteMark",
  "FootnoteReference",
]);
const segmenter = new Intl.Segmenter(undefined, { granularity: "word" });

export type Word = { from: number; to: number; word: string };

/**
 * Words worth checking between `from` and `to`, outside code, addresses and the frontmatter. Tags,
 * mentions, acronyms and words with digits, dots or slashes are names, not spelling.
 */
export function proseWords(state: EditorState, from: number, to: number): Word[] {
  const { doc } = state;
  const meta = frontmatter((n) => doc.line(n).text, doc.lines);
  const skip: [number, number][] = meta ? [[0, doc.line(meta.end).to]] : [];
  syntaxTree(state).iterate({
    from,
    to,
    enter: (node) => {
      if (!notProse.has(node.name)) return;
      skip.push([node.from, node.to]);
      return false;
    },
  });
  skip.sort((a, b) => a[0] - b[0]);
  const words: Word[] = [];
  let at = from;
  for (const [start, end] of [...skip, [to, to] as [number, number]]) {
    if (start > at) collect(state, at, Math.min(start, to), words);
    at = Math.max(at, end);
    if (at >= to) break;
  }
  return words;
}

function collect(state: EditorState, from: number, to: number, words: Word[]) {
  const text = state.sliceDoc(from, to);
  for (const { segment, index, isWordLike } of segmenter.segment(text)) {
    if (!isWordLike) continue;
    const start = from + index,
      end = start + segment.length;
    const before = state.sliceDoc(start - 1, start),
      after = state.sliceDoc(end, end + 1);
    if (
      /[\d_./\\@]/.test(segment) ||
      /[#@/\\]/.test(before) ||
      /[/\\@]/.test(after) ||
      !/\p{L}/u.test(segment) ||
      (segment.length > 1 && segment === segment.toUpperCase())
    )
      continue;
    words.push({ from: start, to: end, word: segment });
  }
}

/** Answers for this session, shared by every editor: true for a misspelled word. */
const answers = new Map<string, boolean>();
const asked = new Set<string>();
const answered = StateEffect.define<null>();
const misspelled = Decoration.mark({ class: "cm-misspelled" });

export type Misspelling = Word & { x: number; y: number };

/** The user added the word to the dictionary: it stops being marked in every editor. */
export function learned(view: EditorView, word: string) {
  answers.set(word, false);
  view.dispatch({ effects: answered.of(null) });
}

/**
 * Marks misspelled words in view, asking `check` about new ones in batches. The word being typed is
 * left alone until the cursor leaves it. A right-click on a marked word calls `menu` instead of
 * opening the native menu.
 */
export function spelling(
  check: (words: string[]) => Promise<boolean[]>,
  menu: (target: Misspelling) => void,
) {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      typing: number | null = null;
      pending = new Set<string>();
      timer: ReturnType<typeof setTimeout> | undefined;
      destroyed = false;
      constructor(readonly view: EditorView) {
        this.decorations = this.build();
      }
      update(update: ViewUpdate) {
        const typed = update.transactions.some(
          (tr) => tr.isUserEvent("input.type") || tr.isUserEvent("delete"),
        );
        const wasTyping = this.typing !== null;
        if (update.docChanged) this.typing = typed ? update.state.selection.main.head : null;
        else if (update.selectionSet) this.typing = null;
        if (
          update.docChanged ||
          update.viewportChanged ||
          (wasTyping && this.typing === null) ||
          update.transactions.some((tr) => tr.effects.some((effect) => effect.is(answered)))
        )
          this.decorations = this.build();
      }
      build(): DecorationSet {
        const marks = [];
        const { state } = this.view;
        for (const { from, to } of this.view.visibleRanges)
          for (const word of proseWords(state, from, to)) {
            if (this.typing !== null && word.from <= this.typing && this.typing <= word.to)
              continue;
            const answer = answers.get(word.word);
            if (answer) marks.push(misspelled.range(word.from, word.to));
            else if (answer === undefined && !asked.has(word.word)) this.pending.add(word.word);
          }
        if (this.pending.size > 0) {
          clearTimeout(this.timer);
          this.timer = setTimeout(() => this.ask(), 300);
        }
        return Decoration.set(marks, true);
      }
      ask() {
        const words = [...this.pending].filter((word) => !answers.has(word) && !asked.has(word));
        this.pending.clear();
        if (words.length === 0) return;
        for (const word of words) asked.add(word);
        check(words)
          .catch((error) => {
            // Unanswered words read as correct for the session, rather than being asked forever.
            console.error("[spelling]", error);
            return [];
          })
          .then((flags) => {
            words.forEach((word, index) => {
              answers.set(word, flags[index] === true);
              asked.delete(word);
            });
            if (!this.destroyed) this.view.dispatch({ effects: answered.of(null) });
          });
      }
      destroy() {
        this.destroyed = true;
        clearTimeout(this.timer);
      }
    },
    {
      decorations: (plugin) => plugin.decorations,
      eventHandlers: {
        contextmenu(event, view) {
          const at = view.posAtCoords({ x: event.clientX, y: event.clientY });
          if (at === null) return false;
          let target: Word | null = null;
          this.decorations.between(at, at, (from, to) => {
            target = { from, to, word: view.state.sliceDoc(from, to) };
            return false;
          });
          if (!target) return false;
          event.preventDefault();
          menu({ ...(target as Word), x: event.clientX, y: event.clientY });
          return true;
        },
      },
    },
  );
}
