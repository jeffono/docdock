# 0011 — Reader view

**Context:** The panel is narrow, and page clutter costs more there than in a browser. M9 asks for a
reader view that shows the article of the loaded page in the existing note renderer and saves it
whole as a note. The page is untrusted ([0004](0004-untrusted-content.md)) and has no channel to
Rust; rendered content is an allowlisted model of DOM nodes, never HTML
([0007](0007-one-markdown-parser.md)). Extraction needs a Readability implementation and an HTML to
Markdown converter.

Candidates (2026-11): `dom_smoothie` 0.19 (MIT, a maintained port of Mozilla's Readability.js on
`dom_query`/`html5ever`, with Markdown output built in), `readability` 0.3 (MIT, an older arc90 port
that pulls `reqwest` by default), `readability-rust` 0.1 (Apache-2.0, a first release),
`llm_readability` 0.0.x (pre-release) and, for conversion, `htmd` (Apache-2.0) and `html2md`
(GPL-3.0, incompatible with the MIT license).

**Decision:**

- **Library:** `dom_smoothie` extracts the article; `dom_query`, which it already uses, writes the
  Markdown. It is a direct dependency only to choose the skipped tags; no other crate is added, and
  `htmd` is not needed.
- **Reading:** Rust reads the live DOM through the same isolated-world `Runtime.evaluate` as the page
  selection. The fixed script copies the document node by node into one without a browsing context,
  so no page code runs for the copy, and never copies scripts, styles, media and embeds (JSON-LD
  stays for metadata). Lengths are counted before anything is copied: the copy ends at 100,000
  elements or 1,500,000 UTF-16 units of markup and text, reads at most 100 attributes per element,
  and answers empty for element nesting deeper than 512, so a huge page costs neither renderer memory
  nor a full walk. The HTML is still cut at 1,500,000 units, since escaping can lengthen it. The
  response is accepted only as two strings within its byte bound, with the 5-second limit.
- **Extraction bounds:** nesting is checked again in Rust before parsing goes further, at most 200,000
  elements are scored (parsing the copy can add some, such as `tbody`), and the Markdown is cut at the last paragraph break within 1 MiB, half the
  note size limit. Controls other than newline and tab are removed.
- **Links and images:** only http(s) and mailto links keep their destination; script, data and
  in-page anchor links become their text. Images are dropped: the CSP blocks remote images, and their
  alt text read as noise. A later task can bring images once they can be shown.
- **Rendering:** the UI renders the article as a note without a path: the frontmatter title becomes
  the header, and image syntax never resolves to `note-asset`. The text it shows is exactly what
  "Save page as note" writes, through the existing create and atomic save path.

**Consequences:**

- The Markdown is untrusted text, like any synced note: the renderer's allowlist is the barrier, and
  Rust's link filtering only keeps saved notes clean.
- 21 new entries in `Cargo.lock`, most a second, newer version of a crate Tauri already uses
  (`html5ever`, `cssparser`, `selectors`, `phf`, `string_cache`). A 350 KB Wikipedia article is
  extracted in about 200 ms in a debug build, off the async runtime.
- The script and the extraction drop shadow DOM content and iframes; pages that need them read
  partially or not at all.
- Replacing the library touches only `reader.rs`; the UI receives Markdown either way.
