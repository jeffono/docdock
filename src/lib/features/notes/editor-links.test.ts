import { markdown } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";
import { expect, it } from "vitest";
import { editorLinkAt } from "./editor-links";
import { markdownExtensions } from "./markdown";

function at(text: string, cursor: string) {
  const state = EditorState.create({
    doc: text,
    extensions: markdown({ extensions: markdownExtensions }),
  });
  return editorLinkAt(state, text.indexOf(cursor), 1)?.href ?? null;
}

it("finds an inline link inside a formatted label", () => {
  expect(at("[**label**](https://example.com)", "label")).toBe("https://example.com/");
});

it("resolves a reference link using its definition", () => {
  expect(at("[label][ref]\n\n[ref]: https://example.com", "label")).toBe("https://example.com/");
});

it("recognizes an autolink", () => {
  expect(at("<https://example.com>", "example")).toBe("https://example.com/");
});

it("recognizes a bare URL", () => {
  expect(at("See https://example.com.", "example")).toBe("https://example.com/");
});

it.each([
  "[label](../other.md)",
  "[label](file:///C:/notes/private.md)",
  "[label](javascript:alert%281%29)",
  "![label](https://example.com/image.png)",
  "`[label](https://example.com)`",
])("does not activate %s", (text) => expect(at(text, "label")).toBeNull());
