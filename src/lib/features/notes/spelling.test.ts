import { markdown } from "@codemirror/lang-markdown";
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { markdownExtensions } from "./markdown";
import { proseWords } from "./spelling";

function words(text: string) {
  const state = EditorState.create({
    doc: text,
    extensions: markdown({ extensions: markdownExtensions }),
  });
  ensureSyntaxTree(state, state.doc.length, 1000);
  return proseWords(state, 0, state.doc.length).map((word) => word.word);
}

// A false mark on every code sample or address would make the checker unbearable.
describe("prose words", () => {
  it("skips code, addresses and the frontmatter", () => {
    const text = [
      "---",
      "title: Ttitle",
      "---",
      "Plain wrogn `codde` [lnk](https://exmple.com/pth) <https://auto.exmple>",
      "```",
      "fencedd",
      "```",
      "    indentedd",
      "",
      "Last",
    ].join("\n");
    expect(words(text)).toEqual(["Plain", "wrogn", "lnk", "Last"]);
  });

  it("skips tags, mentions, acronyms, paths and words with digits", () => {
    expect(words("see #tagg @persn NASA src/fiel.ts v2rc word file.md don't")).toEqual([
      "see",
      "word",
      "don't",
    ]);
  });
});
