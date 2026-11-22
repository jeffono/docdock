import { ensureSyntaxTree } from "@codemirror/language";
import { highlightTree } from "@lezer/highlight";
import { expect, test } from "vitest";
import { codeHighlighter, codeLanguage, codeSpans } from "./code-colors";
import { editorExtensions, editorState } from "./editor";
import { type Content, type Element, noteModel, noteName, sourceRange } from "./render";

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

test("highlighted code preserves hostile text using only fixed-class spans", async () => {
  await codeLanguage("html")?.load();
  const source = '<script>alert(1)</script><img src=x onerror="alert(1)"> &amp;';
  const model = noteModel(`\x60\x60\x60html\n${source}\n\x60\x60\x60`, "a.md");
  const text = (items: Content[]): string =>
    items
      .map((item) => {
        if (typeof item === "string") return item;
        expect(["pre", "code", "span"]).toContain(item.tag);
        if (item.tag === "span")
          expect(item.attrs).toEqual({
            class: expect.stringMatching(
              /^code-(comment|keyword|string|constant|function|type|tag)$/,
            ),
          });
        else expect(item.attrs).toEqual({});
        return text(item.children ?? []);
      })
      .join("");
  expect(text(model)).toBe(source);
});

test("unknown and unlabeled fences remain plain without interpreting their contents", () => {
  for (const info of ["", "unknown", 'html" onclick="alert(1)', "__proto__"]) {
    expect(
      noteModel(`\x60\x60\x60${info}\n<img src=x onerror=alert(1)>\n\x60\x60\x60`, "a.md"),
    ).toEqual([
      {
        tag: "pre",
        attrs: {},
        children: [{ tag: "code", attrs: {}, children: ["<img src=x onerror=alert(1)>"] }],
      },
    ]);
  }
});

test("editor fences and reading use identical language spans without coloring surrounding Markdown", async () => {
  await codeLanguage("TS extra metadata")?.load();
  const code = 'const count: number = 42; // keep\nconst label = "ok";';
  const prefix = "# Plain heading\n\n\x60\x60\x60ts\n";
  const state = editorState(
    `${prefix}${code}\n\x60\x60\x60`,
    editorExtensions(
      () => {},
      () => {},
      "Note",
    ),
  );
  const tree = ensureSyntaxTree(state, state.doc.length, 1000);
  expect(tree).not.toBeNull();
  const spans: ReturnType<typeof codeSpans> = [];
  if (tree)
    highlightTree(tree, codeHighlighter, (from, to, className) =>
      spans.push({ from: from - prefix.length, to: to - prefix.length, className }),
    );
  const reading = codeSpans(code, "typescript");
  expect(reading.length).toBeGreaterThan(0);
  expect(spans).toEqual(reading);
});

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
    '<p>e f <img src="http://note-asset.localhost/sub/img/a%20b.png" alt="g" loading="lazy" data-image="sub/img/a b.png"></img></p>',
  );
});

test("a single image stays in its paragraph and loads lazily", () => {
  expect(html(noteModel("![River](river.png)", "sub/note.md"))).toBe(
    '<p><img src="http://note-asset.localhost/sub/river.png" alt="River" loading="lazy" data-image="sub/river.png"></img></p>',
  );
});

test.each([" ", "\n", "  \n", "\\\n", " <br> ", "<br />", "&nbsp;"])(
  "local images separated by %j render as thumbnail buttons",
  (separator) => {
    const first = '![River](river.png "Wide view")';
    const second = "![](../img/blue%20sky.png)";
    const third = '![](field.png "")';
    const prefix = "---\ntags: [photos]\n---\n\n";
    const paragraph = [first, second, third].join(separator);
    const grid = noteModel(prefix + paragraph, "sub/note.md")[1] as Element;
    expect(html([grid])).toBe(
      '<div class="image-grid">' +
        '<button type="button" class="image-thumb" data-image="sub/river.png" aria-label="River">' +
        '<img src="http://note-asset.localhost/sub/river.png" alt="River" loading="lazy" data-image="sub/river.png" title="Wide view"></img></button>' +
        '<button type="button" class="image-thumb" data-image="img/blue sky.png" aria-label="blue sky.png">' +
        '<img src="http://note-asset.localhost/img/blue%20sky.png" alt="" loading="lazy" data-image="img/blue sky.png"></img></button>' +
        '<button type="button" class="image-thumb" data-image="sub/field.png" aria-label="field.png">' +
        '<img src="http://note-asset.localhost/sub/field.png" alt="" loading="lazy" data-image="sub/field.png" title=""></img></button>' +
        "</div>",
    );
    const from = prefix.length + first.length + separator.length;
    const thumb = grid.children?.[1] as Element;
    expect(sourceRange(grid)).toEqual({
      from: prefix.length,
      to: prefix.length + paragraph.length,
    });
    expect(sourceRange(thumb)).toEqual({ from, to: from + second.length });
    expect(sourceRange(thumb.children?.[0] as Element)).toEqual(sourceRange(thumb));
  },
);

test.each([
  ["captions", "captions"],
  ["![Remote](https://example.com/remote.png)", "Remote"],
  ["![](../../outside.png)", ""],
  ["[![Linked](local.png)](javascript:alert)", 'alt="Linked"'],
  ['<br onclick="alert(1)">', '<br onclick="alert(1)">'],
])("images next to %s stay in a regular paragraph", (middle, visible) => {
  const rendered = html(noteModel(`![](a.png) ${middle} ![](b.png)`, "sub/note.md"));
  expect(rendered).toMatch(/^<p>.*<\/p>$/);
  expect(rendered).toContain(visible);
  expect(rendered).not.toContain("image-grid");
});

test("content without a note path, such as a page article, never shows local images", () => {
  expect(html(noteModel("![a](img/a.png) ![b](https://x.com/b.png)", null))).toBe("<p>a b</p>");
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

// Bug: an untitled note left with text would keep its Untitled name, or take markup as its name.
test("an untitled note is named by its frontmatter title, else its top heading, else its first words", () => {
  expect(noteName("---\ntitle: 'Plan: Q3'\n---\n# Other")).toEqual({
    title: "Plan: Q3",
    frontmatter: true,
  });
  expect(noteName("---\ntags: [a]\n---\n\n# **Bold** idea\n\ntext")).toEqual({
    title: "Bold idea",
    frontmatter: false,
  });
  expect(noteName("Intro\n\n# Later")?.title).toBe("Intro Later");
  expect(noteName("- one\n- [two](https://e.com)\n\n> three")?.title).toBe("one two three");
});

test("an untitled note's name from its words is cut at a word near 30 characters", () => {
  const long = "word ".repeat(30);
  expect(noteName(long)?.title).toBe("word ".repeat(6).trim());
  expect(noteName("x".repeat(80))?.title).toBe("x".repeat(30));
  // Bug: a cut through an emoji left half of it, which Rust rejects as a path.
  expect(noteName(`${"a".repeat(29)}😀xx`)?.title).toBe(`${"a".repeat(29)}😀`);
  expect(noteName("---\ntags: [a]\n---\n\n![](a.png)\n")).toBeNull();
});

test("thumbnail labels do not name the note or hide its first heading", () => {
  const images = "![River](river.png)\n![](field.png)";
  expect(noteName(images)).toBeNull();
  expect(noteName(`${images}\n\n# Weekend walk\n\nSunny day`)?.title).toBe("Weekend walk");
});
