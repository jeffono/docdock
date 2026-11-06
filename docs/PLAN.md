# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

- M4 on `m4-web-pages`: browser mode and unified address bar implemented; code colors on `code-colors`.

## Completed

- M0 — Panel proven and approved with Tauri; see `docs/decisions/0003-panel-and-web-pages.md`.
- M1 — Browse markdown notes, tray, global shortcut, CSP and visual identity; see `docs/DESIGN.md`.
- M2 — Create and edit: atomic autosave, external changes, leftover recovery, frontmatter, editor and
  pasted images.
- M2.1 — Editor polish: save robustness, live formatting, editor chrome and one Markdown parser for
  both views; see `docs/decisions/0007-one-markdown-parser.md`.
- M3 — Find and pin: content search, unified bar, pinned cards and note rows, in-place editing, focused
  panel and list keyboard; see "Note list" in `docs/DESIGN.md` and `docs/decisions/0005-pinned-items.md`.

## M4 — Web pages

Product choices (2026-11-06): links in notes open in the panel, Ctrl+click in the default browser; the
browser bar has back/forward for the page history plus a separate button back to notes; saving a page
creates a note with `title` and `url` in frontmatter and the page selection, if any, quoted in the body;
a note with `url:` expands like any note and offers "Open page".

- [x] (standard) Browser mode: a shown page replaces the note list; a "Notes" button hides the page
  window (kept loaded, not destroyed) and shows the list again; back/forward follow the page history and
  are disabled when unavailable; Rust reports URL, title and history state to the UI; links in rendered
  notes open in the panel, Ctrl+click in the default browser. Replaces the M0 globe toggle and `WebBar`.
- [x] (standard) Unified bar as address bar, with local history and suggestions; see
  `docs/decisions/0006-search-suggestions.md`. Level 2 and a settings screen stay out of M4.
- [ ] (standard) Pin pages in `_pinned.md`; pinned web cards open the page
- [x] (advanced) Read the page selection from Rust without letting the page talk to Rust
- [ ] (standard) Save pages as notes with `url:` in frontmatter; note cards with `url:` offer "Open page"
- [ ] (standard) Code colors with CodeMirror parsers
- [ ] (advanced) Test whether page iframes trigger external schemes without passing through `on_navigation`

## M5 — Distribute

- [ ] (basic) README in English, with a link to the pt-BR version
- [ ] (standard) Installer, automatic updates and decision on code signing
- [ ] (advanced) Quote the autostart path: `auto-launch` writes `Run` without quotes

## Later

In no defined order:

- MathML equations with Temml; KaTeX depends on inline styling, blocked by CSP.
- Mermaid diagrams: require a decision on CSP, which blocks the `<style>` it injects.
- Post-its as floating desktop windows.
- Screenshots.
- Dictation with local models, downloaded separately from the installer.
- Plugins: require a recorded decision before starting.
- macOS and Linux: new `platform/` implementation. On Wayland, the panel depends on layer-shell, which
  GNOME does not provide.

## Open questions

Defaults used until you decide otherwise.

| Question                   | Default                                                                     |
| -------------------------- | --------------------------------------------------------------------------- |
| Monitor                    | The outermost on the chosen side, whose edge holds the cursor               |
| Close when mouse leaves    | Yes, except when pinned, actively editing or the page has focus             |
| Page over full-screen apps | Works in windowed and borderless full-screen; exclusive-mode games excluded |
