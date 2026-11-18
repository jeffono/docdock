import { EditorSelection, Text } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { countSelection, countText, noteBodyStart } from "./count";

describe("editor count", () => {
  it("starts after the closing frontmatter delimiter", () => {
    const doc = Text.of(["---", "title: A note", "---", "Body"]);
    expect(countText(doc.sliceString(noteBodyStart(doc)))).toEqual({ words: 1, chars: 4 });
  });

  it("counts apostrophes and accented letters within words", () => {
    expect(countText("don't ação!").words).toBe(2);
  });

  it("counts emoji and combining sequences as single characters", () => {
    expect(countText("👩‍💻e\u0301").chars).toBe(2);
  });

  it("excludes line breaks but counts spaces", () => {
    expect(countText("a\r\nb\nc\rd\u2028 ").chars).toBe(5);
  });

  it("sums selected ranges independently and clips them to the body", () => {
    const doc = Text.of(["---", "title: Hidden", "---", "One two"]);
    const body = noteBodyStart(doc);
    const selection = EditorSelection.create([
      EditorSelection.range(0, body + 3),
      EditorSelection.cursor(body + 3),
      EditorSelection.range(body + 4, doc.length),
    ]);
    expect(countSelection(doc, selection, body)).toEqual({ words: 2, chars: 6 });
  });
});
