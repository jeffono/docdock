# 0007 — One Markdown parser for reading and editing

**Context:** Reading used `pulldown-cmark` and `ammonia` in Rust; the editor decorates the same text
with the lezer parser in CodeMirror. Each rule (alerts, links, images, frontmatter) existed twice and
the two drifted: an alert with text on its marker line, or `[text]` without a destination, looked
different in each view. Rendering the reading view as a read-only CodeMirror was rejected: it loses
joined paragraphs, formatted copy, semantic HTML and tables.

**Decision:**

- **Parser:** both views use `noteParser` from `features/notes/markdown.ts`: CommonMark, GFM and a
  local footnote extension. Each construct's rule lives once in that module and both views call it.
- **Reading:** `render.ts` turns the syntax tree into a small model of allowlisted tags and
  attributes, then into DOM nodes. Note text only becomes text nodes or attribute values; nothing is
  parsed as HTML, so no sanitizer is needed. `{@html}` is not used.
- **Raw HTML:** inline and block HTML, comments and unknown syntax appear as text in both views.
- **Destinations:** links only for http(s), mailto (including bare addresses) and in-note `#`
  targets; images only through `imageUrl` to `note-asset`. Anything else shows its label as text.
- **Footnotes:** GitHub syntax. A reference renders only when its label is defined; definitions
  render where they are written, with their label, in both views.
- **Rust** no longer renders Markdown: `notes_open` returns the raw text and version only.

**Consequences:**

- `pulldown-cmark` and `ammonia` were removed. The CSP, `note-asset` and navigation rules of
  [0004](0004-untrusted-content.md) are unchanged and remain the outer barrier.
- Rendering runs in the UI; a parser bug can no longer reach Rust commands as HTML, but it can still
  render wrong text, so the model tests cover unsafe destinations and raw HTML.
- Paragraph spacing and soft line breaks still differ by nature: the editor shows source lines.
- Tables render in reading and remain raw in the editor until a table widget exists.
