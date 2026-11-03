# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

Editor polish before M3: tasks 1 and 2 in parallel, then 3.

## Completed

- M0 — Panel proven and approved with Tauri; see `docs/decisions/0003-panel-and-web-pages.md`.
- M1 — Browse markdown notes, tray, global shortcut, CSP and visual identity; see `docs/DESIGN.md`.
- M2 — Create and edit: atomic autosave, external changes, leftover recovery, frontmatter, editor and
  pasted images.

## M2.1 — Editor polish

- [x] (advanced) Save robustness: an external reload can be undone; a note deleted while editing is
  recreated on the next edit, never earlier and never over another file; follow renames; when saving is
  impossible, offer discarding at once
- [ ] (advanced) Live formatting close to reading: one shared content style, GFM, strikethrough, quotes,
  lists, rules, code blocks, task checkboxes, images, GitHub alerts (`[!NOTE]`) in both views, and
  frontmatter as a framed block or the title and tags header. Tables stay raw for now
- [ ] (standard) Editor chrome: floating edit button with save status, accent border while editing,
  standard formatting shortcuts, Ctrl+E, stronger tag contrast in the light theme

## M3 — Find and pin

- [ ] (standard) Content search with an in-memory index; FTS5 only if slowness is measured
- [ ] (standard) Unified bar: search notes and create a note with the typed text, as in Notational Velocity
- [ ] (standard) Pinned notes in an accordion at the top, open by default; see
  `docs/decisions/0005-pinned-items.md`
- [ ] (standard) Restore deleted `_pinned.md`

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
