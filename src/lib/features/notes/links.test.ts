import { describe, expect, it } from "vitest";
import { movedLinks } from "./links";
import { imageUrl } from "./markdown";

describe("movedLinks", () => {
  it("points relative links at the same files from a deeper or shallower folder", () => {
    expect(movedLinks("![](attachments/a.png)", "n.md", "x/y/n.md")).toBe(
      "![](../../attachments/a.png)",
    );
    expect(movedLinks("[o](../other.md)", "x/n.md", "n.md")).toBe("[o](other.md)");
    expect(movedLinks("[o](../b/o.md)", "a/n.md", "b/n.md")).toBe("[o](o.md)");
  });

  it("keeps encoded spaces, angle brackets, titles and fragments", () => {
    expect(movedLinks('![x](my%20pic.png "Title")', "n.md", "a/n.md")).toBe(
      '![x](../my%20pic.png "Title")',
    );
    expect(movedLinks("![x](<my pic.png>)", "n.md", "a/n.md")).toBe("![x](<../my pic.png>)");
    expect(movedLinks("[h](other.md#part)", "n.md", "a/n.md")).toBe("[h](../other.md#part)");
  });

  it("follows reference definitions", () => {
    expect(movedLinks("[x][r]\n\n[r]: img.png 'T'\n", "n.md", "a/n.md")).toBe(
      "[x][r]\n\n[r]: ../img.png 'T'\n",
    );
  });

  it("leaves web, email, anchor, absolute and escaping targets alone", () => {
    const text =
      "[a](https://x.org/p) [b](mailto:a@b.c) [c](#top) [d](www.x.org) [e](/abs.md) [f](../../out.md) [g](C:/x.md)";
    expect(movedLinks(text, "a/n.md", "n.md")).toBe(text);
    expect(movedLinks(text, "n.md", "a/n.md")).toBe(text);
  });

  it("leaves code, the frontmatter, raw HTML and every other byte alone", () => {
    const text =
      '---\r\ntitle: [x](y.md)\r\n---\r\n`[a](b.png)`\r\n\r\n```\r\n![](c.png)\r\n```\r\n<img src="d.png">\r\n\r\n![](e.png)\r\n';
    expect(movedLinks(text, "n.md", "a/n.md")).toBe(text.replace("![](e.png)", "![](../e.png)"));
  });

  it("keeps a moved note's images resolving to the same file", () => {
    const text = "![](attachments/pasted%201.png) ![](../shared/b.png)";
    const moved = movedLinks(text, "a/b/n.md", "c/n.md");
    const sources = (t: string) => [...t.matchAll(/\]\(([^)]+)\)/g)].map((m) => m[1]);
    expect(sources(moved).map((src) => imageUrl(src, "c/n.md"))).toEqual(
      sources(text).map((src) => imageUrl(src, "a/b/n.md")),
    );
  });
});
