import { expect, test } from "vitest";
import { propertyContext } from "./properties";

const at = (text: string, line: number, column: number) => {
  const lines = text.split("\n");
  return propertyContext((n) => lines[n - 1], lines.length, line, column);
};

// Bug: completion would offer keys in the body, or keys the block already has.
test("keys complete at the start of a frontmatter line only, without those already there", () => {
  const text = "---\ntitle: A\nst\n---\nst";
  expect(at(text, 3, 2)).toEqual({ kind: "key", from: 0, used: new Set(["title"]) });
  expect(at(text, 5, 2)).toBeNull();
  expect(at("st\n---", 1, 2)).toBeNull();
  expect(at(text, 2, 7)).toBeNull();
});

// Bug: a tag would replace the whole list, or complete under another key's list.
test("tags complete in inline lists, plain values and items under tags", () => {
  expect(at("---\ntags: [ideia, tra\n---", 2, 20)).toMatchObject({ kind: "tag", from: 17 });
  expect(at("---\ntags: tra\n---", 2, 9)).toMatchObject({ kind: "tag", from: 6 });
  expect(at("---\ntags:\n  - a\n  - tr\n---", 4, 6)).toMatchObject({ kind: "tag", from: 4 });
  expect(at("---\naliases:\n  - tr\n---", 3, 6)).toBeNull();
});
