import { insertBracket } from "@codemirror/autocomplete";
import { redo, undo } from "@codemirror/commands";
import type { Transaction } from "@codemirror/state";
import { type EditorView, keymap } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import { applyTextChanges, editorExtensions, editorState } from "./editor";
import { rawHistory, rawText } from "./reload";

describe("editor text preservation", () => {
  it("preserves mixed line endings and frontmatter when editing and undoing text", () => {
    const raw = "---\r\ntitle: Keep\r\n---\n# Heading\r\nLast\r";
    const state = editorState(raw, []);
    const from = state.doc.toString().indexOf("Heading");
    const edit = state.update({ changes: { from, to: from + 7, insert: "Edited" } });
    const updated = applyTextChanges(raw, edit.changes);
    expect(updated).toBe(raw.replace("Heading", "Edited"));
    expect(applyTextChanges(updated, edit.changes.invert(state.doc))).toBe(raw);
    const newline = state.update({ changes: { from: state.doc.length, insert: "\nNew" } });
    expect(applyTextChanges(raw, newline.changes)).toBe(`${raw}\r\nNew`);
  });
});

// Pairing a multiline selection must not normalize the saved file, even through history.
it("preserves exact line endings when wrapping a selection and undoing the pair", () => {
  const raw = "---\r\ntitle: Keep\r\n---\nFirst\r\nLast\r";
  let state = editorState(raw, [
    ...editorExtensions(
      () => {},
      () => {},
      "Note",
    ),
    rawText.init(() => raw),
    rawHistory,
  ]);
  const from = state.doc.toString().indexOf("First");
  state = state.update({ selection: { anchor: from, head: state.doc.length - 1 } }).state;
  const paired = insertBracket(state, "[");
  if (!paired) throw new Error("Expected bracket pair around the selection");
  state = paired.state;
  const wrapped = raw.replace("First\r\nLast", "[First\r\nLast]");
  expect(state.field(rawText)).toBe(wrapped);
  const dispatch = (transaction: Transaction) => {
    state = transaction.state;
  };
  expect(undo({ state, dispatch })).toBe(true);
  expect(state.field(rawText)).toBe(raw);
  expect(redo({ state, dispatch })).toBe(true);
  expect(state.field(rawText)).toBe(wrapped);
});

// Backspace must remove both halves of an empty pair without breaking list-marker removal.
it("deletes an empty pair in a list before falling back to Markdown Backspace", () => {
  let state = editorState(
    "- ",
    editorExtensions(
      () => {},
      () => {},
      "Note",
    ),
  );
  state = state.update({ selection: { anchor: 2 } }).state;
  const paired = insertBracket(state, "(");
  if (!paired) throw new Error("Expected bracket pair inside the list");
  state = paired.state;
  const view = {
    get state() {
      return state;
    },
    dispatch(transaction: Transaction) {
      state = transaction.state;
    },
  } as EditorView;
  const backspace = () =>
    state
      .facet(keymap)
      .flat()
      .some((binding) => binding.key === "Backspace" && binding.run?.(view));
  expect(backspace()).toBe(true);
  expect(state.doc.toString()).toBe("- ");
  expect(backspace()).toBe(true);
  expect(state.doc.toString()).toBe("");
});
