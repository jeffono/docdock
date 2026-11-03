import { expect, test } from "vitest";
import { type Content, noteModel } from "./render";

const html = (content: Content[]): string =>
  content
    .map((item) => {
      if (typeof item === "string") return item;
      const attrs = Object.entries(item.attrs ?? {})
        .map(([k, v]) => ` ${k}="${v}"`)
        .join("");
      return `<${item.tag}${attrs}>${html(item.children ?? [])}</${item.tag}>`;
    })
    .join("");

test("untrusted markup stays text and only safe destinations become links or images", () => {
  const out = html(
    noteModel(
      [
        '<script>alert(1)</script><img src=x onerror="alert(1)">',
        "",
        "[a](javascript:alert(1)) [b](https://x.com) [c](other.md) <http://y.z> [d](#sec)",
        "",
        "![e](../../out.png) ![f](https://x.com/a.png) ![g](img/a%20b.png)",
      ].join("\n"),
      "sub/note.md",
    ),
  );
  expect(out).toContain(
    '<p class="raw"><script>alert(1)</script><img src=x onerror="alert(1)"></p>',
  );
  expect(out).toContain(
    '<p>a <a href="https://x.com">b</a> c <a href="http://y.z">http://y.z</a> <a href="#sec">d</a></p>',
  );
  expect(out).toContain(
    '<p>e f <img src="http://note-asset.localhost/sub/img/a%20b.png" alt="g"></img></p>',
  );
});

test("alerts follow the GFM rule and footnotes link only when defined", () => {
  const out = html(
    noteModel(
      "> [!warning]\n> Careful.\n\n> [!NOTE] Title\n> Plain.\n\nText[^1] and [^missing].\n\n[^1]: The **note**.",
      "a.md",
    ),
  );
  expect(out).toContain('<blockquote class="markdown-alert-warning"><p>Careful.</p></blockquote>');
  expect(out).toContain("<blockquote><p>[!NOTE] Title\nPlain.</p></blockquote>");
  expect(out).toContain('<p>Text<a class="footnote-ref" href="#fn-1">1</a> and [^missing].</p>');
  expect(out).toContain(
    '<div class="footnote" id="note-fn-1"><span class="footnote-label">1</span> The <strong>note</strong>.</div>',
  );
});

test("frontmatter becomes the header and is never rendered as Markdown", () => {
  const out = html(noteModel("---\r\ntitle: Hi\r\ntags: [a, b]\r\n---\r\n# Body", "a.md"));
  expect(out).toBe(
    '<header class="note-header"><h1>Hi</h1><div class="tags"><span>a</span><span>b</span></div></header><h1>Body</h1>',
  );
});

test("adversarial and edge input renders without hanging, crashing or losing text", () => {
  const started = performance.now();
  noteModel("[^".repeat(32000), "a.md");
  expect(performance.now() - started).toBeLessThan(2000);
  expect(html(noteModel(`${"> ".repeat(3000)}x`, "a.md"))).toContain("x");
  expect(html(noteModel("Text[^a].\n\n[^a]: hello\ncontinued", "a.md"))).toContain(
    '<div class="footnote" id="note-fn-a"><span class="footnote-label">a</span> hello\ncontinued</div>',
  );
  expect(html(noteModel("  ```\n > literal\n  ```\n\n> ```\n>  two\n> ```", "a.md"))).toBe(
    "<pre><code>> literal</code></pre><blockquote><pre><code> two</code></pre></blockquote>",
  );
  expect(
    html(noteModel("[x](https://e.com/?a=1&amp;b=2) [y][]\n\n[y]: https://y.com", "a.md")),
  ).toBe('<p><a href="https://e.com/?a=1&b=2">x</a> <a href="https://y.com">y</a></p>');
  expect(html(noteModel("![i](a&#32;b.png)", "a.md"))).toContain(
    'src="http://note-asset.localhost/a%20b.png"',
  );
});
