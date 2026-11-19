import { EditorState } from "@codemirror/state";
import { expect, test } from "vitest";
import { pointerReveal, releaseReveal, revealed } from "./pointer-reveal";

const start = () =>
  EditorState.create({ doc: "---\ntitle: T\n---\nBody text", extensions: pointerReveal() });
const heads = (state: EditorState) => revealed(state).map((range) => range.head);

test("a pointer selection keeps revealing what the previous selection did", () => {
  const state = start().update({ selection: { anchor: 20 }, userEvent: "select.pointer" }).state;
  expect(heads(state)).toEqual([0]);
});

test("releasing reveals the pointer's selection", () => {
  const held = start().update({ selection: { anchor: 20 }, userEvent: "select.pointer" }).state;
  expect(heads(held.update({ effects: releaseReveal.of(null) }).state)).toEqual([20]);
});

test("a keyboard selection reveals at once, also during a pointer gesture", () => {
  const held = start().update({ selection: { anchor: 20 }, userEvent: "select.pointer" }).state;
  const moved = held.update({ selection: { anchor: 21 }, userEvent: "select" }).state;
  expect(heads(moved)).toEqual([21]);
});

// A reload during a gesture must not leave held positions past the end of a shorter text.
test("text removed during a pointer gesture maps the held selection", () => {
  const held = start()
    .update({ selection: { anchor: 25 } })
    .state.update({ selection: { anchor: 5 }, userEvent: "select.pointer" }).state;
  expect(heads(held.update({ changes: { from: 10, to: 26 } }).state)).toEqual([10]);
});
