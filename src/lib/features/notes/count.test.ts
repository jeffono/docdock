import { EditorSelection, Text } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { countNote, countSelection, countText, formatTimeRange, noteBodyStart } from "./count";

describe("editor count", () => {
  it("starts after the closing frontmatter delimiter", () => {
    const doc = Text.of(["---", "title: A note", "---", "Body"]);
    expect(countText(doc.sliceString(noteBodyStart(doc)))).toEqual({
      words: 1,
      chars: 4,
      images: 0,
    });
  });

  it("excludes frontmatter in a raw note with CRLF line endings", () => {
    expect(countNote("---\r\ntitle: Hidden\r\n---\r\nOne two")).toEqual({
      words: 2,
      chars: 7,
      images: 0,
    });
  });

  it("counts apostrophes and accented letters within words", () => {
    expect(countText("don't ação!").words).toBe(2);
  });

  it("counts emoji and combining sequences as single characters", () => {
    expect(countText("👩‍💻e\u0301").chars).toBe(2);
  });

  it("counts images apart from the prose around them", () => {
    expect(countText("Trip ![a](x.png) ![](attachments/y%20z.jpg)\nday")).toEqual({
      words: 2,
      chars: 9,
      images: 2,
    });
  });

  it("leaves image syntax inside code as text", () => {
    expect(countText("`![a](x.png)`").images).toBe(0);
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
    expect(countSelection(doc, selection, body)).toEqual({ words: 2, chars: 6, images: 0 });
  });
});

describe("time estimates", () => {
  it.each([
    { name: "rounds up fractional seconds", words: 74, fast: 300, slow: 240, expected: "15s~19s" },
    { name: "shows minutes as a clock", words: 400, fast: 300, slow: 192, expected: "01:20~02:05" },
    {
      name: "adds hours to the clock",
      words: 19500,
      fast: 300,
      slow: 216,
      expected: "1:05:00~1:30:17",
    },
    {
      name: "collapses matching seconds to one value",
      words: 1,
      fast: 300,
      slow: 175,
      expected: "1s",
    },
    { name: "omits estimates for zero words", words: 0, fast: 300, slow: 175, expected: "" },
  ])("$name", ({ words, fast, slow, expected }) => {
    expect(formatTimeRange(words, fast, slow)).toBe(expected);
  });
});
