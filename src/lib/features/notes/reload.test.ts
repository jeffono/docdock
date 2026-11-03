import { history, redo, undo } from "@codemirror/commands";
import { EditorState, Transaction } from "@codemirror/state";
import { expect, it } from "vitest";
import { diskReload, rawHistory, rawText } from "./reload";

it("undoes a disk reload as one step and recovers exact mixed line endings", () => {
  const original = "---\r\ntitle: Mine\n---\rBody\r\n";
  let state = EditorState.create({
    doc: original,
    extensions: [history(), rawText.init(() => original), rawHistory],
  });
  const dispatch = (transaction: import("@codemirror/state").Transaction) => {
    state = transaction.state;
  };
  state = state.update({
    changes: { from: 0, insert: "Saved app edit\n" },
    annotations: Transaction.userEvent.of("input.type"),
  }).state;
  state = state.update({
    changes: { from: 15, insert: "Again\n" },
    annotations: Transaction.userEvent.of("input.type"),
  }).state;
  const saved = state.field(rawText);
  state = state.update(diskReload(state, "Replaced\nexternally\r\n")).state;
  expect(state.field(rawText)).toBe("Replaced\nexternally\r\n");
  expect(undo({ state, dispatch })).toBe(true);
  expect(state.field(rawText)).toBe(saved);
  expect(undo({ state, dispatch })).toBe(true);
  expect(state.field(rawText)).toBe(original);
  expect(redo({ state, dispatch })).toBe(true);
  expect(redo({ state, dispatch })).toBe(true);
  expect(state.field(rawText)).toBe("Replaced\nexternally\r\n");
  // Even changes invisible to CodeMirror's normalized document stay undoable.
  state = state.update(diskReload(state, "Replaced\r\nexternally\n")).state;
  expect(undo({ state, dispatch })).toBe(true);
  expect(state.field(rawText)).toBe("Replaced\nexternally\r\n");
});
