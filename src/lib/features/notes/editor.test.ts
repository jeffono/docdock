import { insertBracket } from "@codemirror/autocomplete";
import { isolateHistory, redo, undo } from "@codemirror/commands";
import { Compartment, EditorState, type Transaction } from "@codemirror/state";
import { type EditorView, keymap } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import { applyTextChanges, editorExtensions, editorState } from "./editor";
import { grids, notePath } from "./live-formatting";
import { addProperty } from "./properties";
import { rawHistory, rawText } from "./reload";
import { slashCompletion } from "./slash-commands";

describe("editor text preservation", () => {
  // Emptying the line between a lone CR and an LF break once merged them into one CRLF break.
  it("keeps both breaks when the line between a CR and an LF break becomes empty", () => {
    const raw = "keep\rZ\nA";
    const edit = editorState(raw, []).update({ changes: { from: 5, to: 6 } });
    expect(applyTextChanges(raw, edit.changes).split(/\r\n|\r|\n/)).toEqual(["keep", "", "A"]);
  });

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

describe("decoupled editor options", () => {
  it("disables frontmatter completions and metadata slash command when frontmatter is off", () => {
    const extensions = editorExtensions(
      () => {},
      () => {},
      "Scratchpad",
      () => ["tag"],
      { frontmatter: false, leaveOnCtrlE: false },
    );
    const makeCtx = (doc: string) => ({
      state: editorState(doc, extensions),
      pos: doc.length,
      explicit: true,
      aborted: false,
      addEventListener() {},
      matchBefore: () => null,
    });
    const all = slashCompletion(makeCtx("---\ntitle: Keep\n---\n/") as never);
    expect(all?.options.length).toBeGreaterThan(0);
    expect(all?.options.some((o) => o.label.includes("Add metadata"))).toBe(false);
    expect(slashCompletion(makeCtx("---\ntitle: Keep\n---\n/add") as never)).toBeNull();
    // Ctrl+; and Ctrl+E should not be bound when disabled.
    const bindings = makeCtx("").state.facet(keymap).flat();
    expect(bindings.some((b) => b.run === addProperty)).toBe(false);
    expect(bindings.some((b) => b.key === "Ctrl-e")).toBe(false);
  });

  it("restores exact mixed line endings when clearing and undoing", () => {
    const raw = "First\r\nSecond\n";
    let state = editorState(raw, [
      ...editorExtensions(
        () => {},
        () => {},
        "Scratchpad",
        () => [],
        { frontmatter: false, leaveOnCtrlE: false },
      ),
      rawText.init(() => raw),
      rawHistory,
    ]);
    const dispatch = (transaction: Transaction) => {
      state = transaction.state;
    };
    state = state.update({
      changes: { from: 0, to: state.doc.length, insert: "" },
      selection: { anchor: 0 },
      annotations: isolateHistory.of("full"),
      userEvent: "delete",
    }).state;
    expect(state.field(rawText)).toBe("");
    expect(undo({ state, dispatch })).toBe(true);
    expect(state.field(rawText)).toBe(raw);
  });

  it("records programmatic clear in undo history while read-only so undo works after it lifts", () => {
    const editable = new Compartment();
    const raw = "Promoted draft\n";
    let state = editorState(raw, [
      ...editorExtensions(
        () => {},
        () => {},
        "Scratchpad",
        () => [],
        { frontmatter: false, leaveOnCtrlE: false },
      ),
      rawText.init(() => raw),
      rawHistory,
      editable.of(EditorState.readOnly.of(true)),
    ]);
    const dispatch = (transaction: Transaction) => {
      state = transaction.state;
    };
    state = state.update({
      changes: { from: 0, to: state.doc.length, insert: "" },
      selection: { anchor: 0 },
      annotations: isolateHistory.of("full"),
      userEvent: "delete",
    }).state;
    expect(state.field(rawText)).toBe("");
    // Once read-only lifts (after promotion completes), Ctrl+Z still restores the draft.
    state = state.update({ effects: editable.reconfigure(EditorState.readOnly.of(false)) }).state;
    expect(undo({ state, dispatch })).toBe(true);
    expect(state.field(rawText)).toBe(raw);
  });

  it("resolves no local image grids when notePath is null", () => {
    const doc = "Intro\n\n![a](a.png)\n![b](b.png)";
    const state = editorState(doc, [
      ...editorExtensions(
        () => {},
        () => {},
        "Scratchpad",
        () => [],
        { frontmatter: false },
      ),
      notePath.of(null),
    ]);
    const ranges: [number, number][] = [];
    state.field(grids).between(0, state.doc.length, (from, to) => {
      ranges.push([from, to]);
    });
    expect(ranges).toEqual([]);
  });
});
