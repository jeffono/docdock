import { ensureSyntaxTree } from "@codemirror/language";
import { describe, expect, it } from "vitest";
import { editorExtensions, editorState } from "./editor";
import { grids, notePath } from "./live-formatting";

/** The ranges shown as image grids for `doc`, with the cursor at `cursor`. */
function gridRanges(doc: string, cursor: number) {
  let state = editorState(doc, [
    ...editorExtensions(
      () => {},
      () => {},
      "Note",
    ),
    notePath.of("note.md"),
  ]);
  ensureSyntaxTree(state, state.doc.length, 5000);
  state = state.update({ selection: { anchor: cursor } }).state;
  const ranges: [number, number][] = [];
  state.field(grids).between(0, state.doc.length, (from, to) => {
    ranges.push([from, to]);
  });
  return ranges;
}

const images = "![a](a.png)\n![b](b.png)";

describe("image grids in the editor", () => {
  // Catches a grid hiding the Markdown being edited, or text that shares the paragraph.
  it("shows a paragraph of two or more images as a grid only while the cursor is away", () => {
    const doc = `Intro\n\n${images}\n\nEnd`;
    const start = doc.indexOf("![");
    expect(gridRanges(doc, 0)).toEqual([[start, start + images.length]]);
    expect(gridRanges(doc, start + 3)).toEqual([]);
  });

  it("leaves a single image and a paragraph with text as they are", () => {
    expect(gridRanges("Intro\n\n![a](a.png)", 0)).toEqual([]);
    expect(gridRanges(`Intro\n\n${images} caption`, 0)).toEqual([]);
  });
});
