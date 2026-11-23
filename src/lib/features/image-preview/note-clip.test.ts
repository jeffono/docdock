import { expect, test } from "vitest";
import { frontmatter } from "../notes/markdown";
import { clipPreviewNote } from "./note-clip";

test("returns short notes untouched", () => {
  const source = "---\ntitle: Short\n---\n\nHello\n";
  expect(clipPreviewNote(source)).toBe(source);
});

test("keeps frontmatter intact when truncating long body by line count", () => {
  const body = Array.from({ length: 150 }, (_, i) => `Line ${i + 1}`).join("\n");
  const source = `---\ntitle: Long\ntags: [a]\n---\n${body}`;
  const clipped = clipPreviewNote(source, 20, 12_000);
  const lines = clipped.split("\n");
  expect(lines.slice(0, 4)).toEqual(["---", "title: Long", "tags: [a]", "---"]);
  expect(lines).toHaveLength(24);
  expect(lines.at(-1)).toBe("Line 20");
});

test("closes an unclosed fenced code block after truncation", () => {
  const code = Array.from({ length: 30 }, (_, i) => `const x${i} = ${i};`).join("\n");
  const source = `\`\`\`ts\n${code}\n\`\`\``;
  const clipped = clipPreviewNote(source, 5, 12_000);
  expect(clipped.endsWith("\n```")).toBe(true);
});

test("clips by character budget on a giant single line", () => {
  const clipped = clipPreviewNote("a".repeat(20_000), 100, 1000);
  expect(clipped).toHaveLength(1000);
});

test("bounds oversized frontmatter metadata while keeping title and first tags parseable", () => {
  const tags = Array.from({ length: 100 }, (_, i) => `tag-${i}-${"x".repeat(80)}`);
  const extra = Array.from({ length: 80 }, (_, i) => `extra_${i}: value`).join("\n");
  const source = `---\ntitle: ${"T".repeat(300)}\ntags: [${tags.join(", ")}]\n${extra}\n---\nBody`;
  const clipped = clipPreviewNote(source);
  const lines = clipped.split("\n");
  const meta = frontmatter((n) => lines[n - 1], lines.length);
  expect(meta).not.toBeNull();
  expect(meta?.title).toHaveLength(200);
  expect(meta?.tags).toHaveLength(24);
  expect(meta?.tags[0]).toHaveLength(64);
  expect(lines.at(-1)).toBe("Body");
});
