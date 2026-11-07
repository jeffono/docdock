import { expect, test } from "vitest";
import { yamlValue } from "./markdown";
import { propertyContext } from "./properties";

const at = (text: string, line: number, column: number) => {
  const lines = text.split("\n");
  return propertyContext((n) => lines[n - 1], lines.length, line, column);
};

// Bug: completion would offer keys in the body, or keys the block already has.
test("keys complete at the start of a frontmatter line only, without those already there", () => {
  const text = "---\ntitle: A\nst\n---\nst";
  expect(at(text, 3, 2)).toEqual({
    kind: "key",
    from: 0,
    to: 2,
    colon: false,
    used: new Set(["title"]),
  });
  expect(at(text, 5, 2)).toBeNull();
  expect(at("st\n---", 1, 2)).toBeNull();
  expect(at(text, 2, 7)).toBeNull();
});

// Bug: completing a key before its colon gave `title: : A`.
test("a key completed before its colon is replaced whole and keeps the colon", () => {
  expect(at("---\ntitle: A\n---", 2, 2)).toMatchObject({
    kind: "key",
    from: 0,
    to: 5,
    colon: true,
  });
});

// Bug: a tag such as "Plano: Q3" or "a,b" was inserted as YAML that reads as a map or two tags.
test("tags are inserted as YAML values that read back the same", () => {
  expect(yamlValue("ideia")).toBe("ideia");
  expect(yamlValue("Plano: Q3")).toBe('"Plano: Q3"');
  expect(yamlValue("a,b")).toBe('"a,b"');
  expect(yamlValue('di"z')).toBe('"di\\"z"');
});

// Bug: a tag would replace the whole list, or complete under another key's list.
test("tags complete in inline lists, plain values and items under tags", () => {
  expect(at("---\ntags: [ideia, tra\n---", 2, 20)).toMatchObject({ kind: "tag", from: 17 });
  expect(at("---\ntags: tra\n---", 2, 9)).toMatchObject({ kind: "tag", from: 6 });
  expect(at("---\ntags:\n  - a\n  - tr\n---", 4, 6)).toMatchObject({ kind: "tag", from: 4 });
  expect(at("---\naliases:\n  - tr\n---", 3, 6)).toBeNull();
});
