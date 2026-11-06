import { expect, test } from "vitest";
import { frontmatter } from "./markdown";
import { blockquote, pageNote } from "./page";

function metadata(text: string) {
  const lines = text.split("\n");
  return frontmatter((n) => lines[n - 1], lines.length);
}

test("page titles round-trip without injecting YAML keys or delimiters", () => {
  for (const title of [
    "Quotes: \"double\" and 'single' # literal",
    "first\r\n---\nurl: javascript:alert(1)\nlast",
    "- list",
    "> quote",
    "| block",
    "---",
    "true",
    "",
    "back\\slash\tand\u0000controls\u0085\u2028\u2029",
  ]) {
    const note = pageNote({ title, url: "https://example.com/a?q=1#part", selection: "" });
    const meta = metadata(note.text);
    expect(meta?.title).toBe(title || null);
    expect(meta?.url).toBe("https://example.com/a?q=1#part");
    expect(meta?.end).toBe(4);
    expect(note.text.split("\n").slice(4).join("\n")).toBe("\n");
  }
});

test("every selection line stays quoted across line endings, blanks and delimiters", () => {
  expect(blockquote("first\r\n\r\n---\n</blockquote>\rlast\n")).toBe(
    "> first\n>\n> ---\n> </blockquote>\n> last\n>",
  );
  expect(blockquote("")).toBe("");
});

test("frontmatter offers a page destination only for absolute HTTP(S) URLs", () => {
  for (const url of ["https://example.com", "http://example.com/a", "HTTPS://example.com"]) {
    expect(metadata(`---\nurl: ${url}\n---`)?.url).toBe(url);
  }
  for (const url of [
    "javascript:alert(1)",
    "file:///C:/notes",
    "mailto:a@b.com",
    "//example.com",
    "https://",
    "./note.md",
    "",
  ]) {
    expect(metadata(`---\nurl: ${url}\n---`)?.url).toBeNull();
  }
});
