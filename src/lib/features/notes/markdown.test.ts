import { expect, test } from "vitest";
import { imageUrl } from "./markdown";

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
