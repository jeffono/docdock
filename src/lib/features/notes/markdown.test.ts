import { expect, test } from "vitest";
import { imageUrl, retitle } from "./markdown";

test("editor image paths follow Rust decoding, traversal and encoding rules", () => {
  expect(imageUrl("../images/a%20b!'().png", "sub/note.md")).toBe(
    "http://note-asset.localhost/images/a%20b%21%27%28%29.png",
  );
  expect(imageUrl("100%.png", "note.md")).toBe("http://note-asset.localhost/100%25.png");
  expect(imageUrl("%252e%252e/x.png", "note.md")).toBe(
    "http://note-asset.localhost/%252e%252e/x.png",
  );
  for (const src of [
    "../../x.png",
    "%2e%2e/x.png",
    "%2Fabsolute.png",
    "x%3ay.png",
    "a%5cb.png",
    "x%3fq",
    "x%23f",
    "%FF",
    "https://example.com/x.png",
    "",
  ])
    expect(imageUrl(src, "sub/note.md")).toBe(
      src === "%2e%2e/x.png" ? "http://note-asset.localhost/x.png" : null,
    );
});

test("retitle changes only the title line and keeps its terminator", () => {
  const text = "---\r\ntags: [a]\r\ntitle: Old # note\r\n---\r\ntitle: body\r\n";
  expect(retitle(text, "New")).toBe("---\r\ntags: [a]\r\ntitle: New\r\n---\r\ntitle: body\r\n");
});

test("retitle quotes a title that would read back differently or as another type", () => {
  const text = "---\ntitle: Old\n---\n";
  expect(retitle(text, "A: b # c")).toBe('---\ntitle: "A: b # c"\n---\n');
  expect(retitle(text, "2024")).toBe('---\ntitle: "2024"\n---\n');
});

test("retitle leaves notes whose card shows the file name", () => {
  expect(retitle("# Heading\n", "New")).toBeNull();
  expect(retitle("---\ntitle: [list]\n---\n", "New")).toBeNull();
});
