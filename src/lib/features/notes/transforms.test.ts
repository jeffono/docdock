import { history, undo } from "@codemirror/commands";
import { EditorSelection, EditorState, Transaction, type TransactionSpec } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { afterEach, expect, it, vi } from "vitest";
import { applyTextChanges } from "./editor";
import {
  capitalizeWords,
  cycleCase,
  deduplicateLines,
  sentenceCase,
  sortLines,
  textTransforms,
} from "./transforms";

afterEach(() => vi.unstubAllGlobals());

it("capitalizes each sentence while preserving Markdown punctuation", () => {
  expect(sentenceCase("**olá**. próxima FRASE! outra ação? sim.", "pt-BR")).toBe(
    "**Olá**. Próxima frase! Outra ação? Sim.",
  );
});

it("capitalizes accented words and keeps apostrophes within words", () => {
  expect(capitalizeWords("ação e don't grite ÁGUA", "pt-BR")).toBe("Ação E Don't Grite Água");
});

it.each([
  ["olá mundo", "Olá mundo"],
  ["Olá mundo", "Olá Mundo"],
  ["Olá Mundo", "OLÁ MUNDO"],
  ["OLÁ MUNDO", "olá mundo"],
  ["Olá", "OLÁ"],
])("cycles %s to %s", (before, after) => {
  expect(cycleCase(before, "pt-BR")).toBe(after);
});

it("sorts numbers naturally and compares accented letters at base sensitivity", () => {
  const text = "bola\nÁrvore 10\nárvore 2\nArvore 1";
  expect(sortLines(text, "pt-BR", 1)).toBe("Arvore 1\nárvore 2\nÁrvore 10\nbola");
  expect(sortLines(text, "pt-BR", -1)).toBe("bola\nÁrvore 10\nárvore 2\nArvore 1");
});

it("deduplicates exact lines in order of first occurrence", () => {
  expect(deduplicateLines("árvore\nB\narvore\nárvore\nb\nB")).toBe("árvore\nB\narvore\nb");
});

function editor(raw: string, selection: EditorSelection) {
  vi.stubGlobal("document", { documentElement: { lang: "pt-BR" } });
  let state = EditorState.create({
    doc: raw,
    selection,
    extensions: [history(), EditorState.allowMultipleSelections.of(true)],
  });
  return {
    get state() {
      return state;
    },
    get raw() {
      return raw;
    },
    dispatch(spec: Transaction | TransactionSpec) {
      const transaction = spec instanceof Transaction ? spec : state.update(spec);
      raw = applyTextChanges(raw, transaction.changes);
      state = transaction.state;
    },
  };
}

it("sorts whole lines as one undo step and preserves mixed line endings", () => {
  const raw = "item 10\r\nitem 2\nitem 1\rkeep";
  const view = editor(raw, EditorSelection.single(2, "item 10\nitem 2\nitem 1\n".length));
  textTransforms.find(({ id }) => id === "sort-ascending")?.run(view as unknown as EditorView);
  expect(view.raw).toBe("item 1\r\nitem 2\nitem 10\rkeep");
  const { from, to } = view.state.selection.main;
  expect(view.state.sliceDoc(from, to)).toBe("item 1\nitem 2\nitem 10");
  expect(undo(view)).toBe(true);
  expect(view.raw).toBe(raw);
  expect(undo(view)).toBe(false);
});

it("transforms each target once when a cursor's word overlaps another selection", () => {
  const selection = EditorSelection.create([
    EditorSelection.range(1, 2),
    EditorSelection.cursor(3),
    EditorSelection.cursor(8),
  ]);
  const view = editor("ação don't", selection);
  textTransforms.find(({ id }) => id === "capitalize-words")?.run(view as unknown as EditorView);
  expect(view.raw).toBe("Ação Don't");
  expect(view.state.selection.ranges.map(({ from, to }) => view.state.sliceDoc(from, to))).toEqual([
    "Ação",
    "Don't",
  ]);
});

it("expands single-line JSON across multiple lines and restores on undo", () => {
  const raw = '{"b":1,"a":true}';
  const view = editor(raw, EditorSelection.single(0, raw.length));
  textTransforms.find(({ id }) => id === "format-json")?.run(view as unknown as EditorView);
  expect(view.raw).toBe('{\n  "b": 1,\n  "a": true\n}');
  expect(undo(view)).toBe(true);
  expect(view.raw).toBe(raw);
});

it("preserves boundary whitespace when normalizing partial inline selections", () => {
  const viewA = editor("hello world", EditorSelection.single(5, 11));
  textTransforms
    .find(({ id }) => id === "normalize-whitespace")
    ?.run(viewA as unknown as EditorView);
  expect(viewA.raw).toBe("hello world");

  const viewB = editor("hello   world  foo", EditorSelection.single(5, 15));
  textTransforms
    .find(({ id }) => id === "normalize-whitespace")
    ?.run(viewB as unknown as EditorView);
  expect(viewB.raw).toBe("hello world foo");
});
