import { expect, test } from "vitest";
import { sourceOffset } from "./reading-position";
import { type Content, type Element, noteModel } from "./render";

/** The block, path and offset of the `nth` text in the model that contains `word`. */
function point(model: Content[], word: string, nth = 0) {
  const found: { block: Element; path: number[]; offset: number }[] = [];
  const visit = (block: Element, item: Content, path: number[]) => {
    if (typeof item === "string") {
      for (let at = item.indexOf(word); at !== -1; at = item.indexOf(word, at + 1))
        found.push({ block, path, offset: at });
      return;
    }
    item.children?.forEach((child, i) => {
      visit(block, child, [...path, i]);
    });
  };
  for (const block of model) if (typeof block !== "string") visit(block, block, []);
  return found[nth];
}

function offsetOf(text: string, word: string, nth = 0) {
  const { block, path, offset } = point(noteModel(text, "a.md"), word, nth);
  return sourceOffset(text.replace(/\r\n?/g, "\n"), block, path, offset);
}

test("a word after a link is found after the link's hidden address", () => {
  const text = "See [cat](cat.md) cat here";
  expect(offsetOf(text, "cat", 1)).toBe(text.lastIndexOf("cat"));
});

test("a word in a link's label is found in the label", () => {
  const text = "See [cat](cat.md) cat here";
  expect(offsetOf(text, "cat")).toBe(text.indexOf("cat"));
});

test("body positions count from the note's start, with line breaks as the editor counts them", () => {
  const text = "---\r\ntitle: T\r\n---\r\nHello world";
  expect(offsetOf(text, "world")).toBe(text.replace(/\r\n/g, "\n").indexOf("world"));
});

test("code is found after its fence's language name", () => {
  const text = "```javascript\nscript()\n```";
  expect(offsetOf(text, "script")).toBe(text.indexOf("script()"));
});

test("a frontmatter title is found in its value, not in its key", () => {
  const text = "---\ntitle: it works\n---\nBody";
  expect(offsetOf(text, "it")).toBe(text.indexOf("it works"));
});
