# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

Nothing. Next: frontmatter and the editor, in parallel; then pasting images.

## Completed

- M0 — Panel proven and approved with Tauri; see `docs/decisions/0003-panel-and-web-pages.md`.
- M1 — Browse markdown notes, tray, global shortcut, CSP and visual identity; see `docs/DESIGN.md`.

## Current stage: M2 — Create and edit

- [x] (advanced) Screen side configurable from the tray, right by default; independent of the rest
- [x] (standard) Internationalize the UI and Rust text: English by default, pt-BR based on the Windows
  language; Rust errors become codes
- [x] (advanced) Atomic writes and autosave
- [x] (advanced) React to external changes without content loss
- [x] (standard) Recover `.docdock-bak` leftovers from a save interrupted between its two renames: list them
  on opening the folder and offer to restore, never deleting automatically
- [ ] (standard) Frontmatter: hide the block and show title and tags
- [ ] (standard) CodeMirror 6 editor with live formatting: headings, emphasis and markup hidden outside
  the current line. No WYSIWYG, which would rewrite the file. On an external change, reload the text only
  without unsaved edits; otherwise keep them and show the conflict
- [ ] (standard) Paste images as attachments

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
