# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

Nothing. Next: close M3 (test review and cross-provider review).

## Completed

- M0 — Panel proven and approved with Tauri; see `docs/decisions/0003-panel-and-web-pages.md`.
- M1 — Browse markdown notes, tray, global shortcut, CSP and visual identity; see `docs/DESIGN.md`.
- M2 — Create and edit: atomic autosave, external changes, leftover recovery, frontmatter, editor and
  pasted images.
- M2.1 — Editor polish: save robustness, live formatting, editor chrome and one Markdown parser for
  both views; see `docs/decisions/0007-one-markdown-parser.md`.

## M3 — Find and pin

- [x] (standard) Content search with an in-memory index; FTS5 only if slowness is measured
- [x] (standard) Unified bar at the top: search notes and create a note with the typed text, as in
  Notational Velocity
- [x] (standard) Note list with Pinned cards and Notes rows, card states and keyboard; see "Note list" in
  `docs/DESIGN.md` and `docs/decisions/0005-pinned-items.md`
- [x] (standard) In-place editing in an expanded card, replacing the separate note view
- [x] (standard) Restore deleted `_pinned.md`

## M4 — Web pages

- [ ] (standard) Browser in place of notes, with a back button; the page is hidden, not closed
- [ ] (standard) Unified bar as address bar, with local history and suggestions; see
  `docs/decisions/0006-search-suggestions.md`
- [ ] (standard) Pin pages in `_pinned.md`
- [ ] (standard) Save pages as notes with `url:` in frontmatter
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
