import { EditorState } from "@codemirror/state";
import { expect, test } from "vitest";
import { joinLines, toggleInline } from "./formatting";

test.each(["**", "*", "~~", "`"])(
  "inline %s toggle removes only matching pairs and preserves surrounding user text",
  (marker) => {
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
  },
);

// Catches content loss or leftover Markdown prefixes when joining editor lines.
test.each(["- ", "* ", "+ ", "1. ", "2) ", "- [ ] ", "* [x] ", "[X] ", "> ", ">   "])(
  "join removes the following line's %s prefix and indentation",
  (prefix) => {
    expect(joinLines(`first\n  ${prefix}second\nlast`, 2, 2)).toEqual({
      from: 0,
      to: 14 + prefix.length,
      insert: "first second",
    });
  },
);
test("join spans all selected lines and leaves surrounding lines untouched", () => {
  const text = "before\nfirst\n- second\n> third\nafter";
  const change = joinLines(text, 9, 26);
  expect(EditorState.create({ doc: text }).update({ changes: change }).state.doc.toString()).toBe(
    "before\nfirst second third\nafter",
  );
});
test.each([
  ["\n  next", "next"],
  ["first \n next", "first next"],
  ["first\n next", "first next"],
  ["last", "last"],
])("join spacing preserves existing whitespace in %j", (text, insert) => {
  expect(joinLines(text, 0, 0).insert).toBe(insert);
});
