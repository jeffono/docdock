import { EditorState } from "@codemirror/state";
import { expect, test } from "vitest";
import { toggleInline } from "./formatting";

test("inline toggles remove only matching pairs and preserve surrounding user text", () => {
  for (const marker of ["**", "*", "~~", "`"]) {
    const plain = "before text after";
    const wrapped = `before ${marker}text${marker} after`;
    const apply = (text: string, from: number, to: number) => {
      const edit = toggleInline(text, from, to, marker);
      return EditorState.create({ doc: text }).update({
        changes: edit.changes,
        selection: { anchor: edit.anchor, head: edit.head },
      }).state;
    };
    const added = apply(plain, 7, 11);
    expect(added.doc.toString()).toBe(wrapped);
    expect(added.sliceDoc(added.selection.main.from, added.selection.main.to)).toBe("text");
    expect(apply(wrapped, 7, 11 + marker.length * 2).doc.toString()).toBe(plain);
    expect(apply(wrapped, 7 + marker.length, 11 + marker.length).doc.toString()).toBe(plain);
    const empty = apply(plain, 7, 7);
    expect(empty.doc.toString()).toBe(`before ${marker}${marker}text after`);
    expect(empty.selection.main.head).toBe(7 + marker.length);
    expect(
      apply(
        empty.doc.toString(),
        empty.selection.main.head,
        empty.selection.main.head,
      ).doc.toString(),
    ).toBe(plain);
    const unmatched = `before ${marker}text after`;
    expect(apply(unmatched, 7 + marker.length, 11 + marker.length).doc.toString()).toBe(
      `before ${marker}${marker}text${marker} after`,
    );
    const multiline = "before first\nsecond after";
    expect(apply(multiline, 7, 19).doc.toString()).toBe(
      `before ${marker}first\nsecond${marker} after`,
    );
  }
});
