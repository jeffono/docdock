import { describe, expect, it } from "vitest";
import { applyTextChanges, editorState } from "./editor";

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
