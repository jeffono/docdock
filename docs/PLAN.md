# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

- M4.1 on `preferences`: Preferences view and width done; note order on `note-order` (Codex worktree).

## Completed

- M0 — Panel proven and approved with Tauri; see `docs/decisions/0003-panel-and-web-pages.md`.
- M1 — Browse markdown notes, tray, global shortcut, CSP and visual identity; see `docs/DESIGN.md`.
- M2 — Create and edit: atomic autosave, external changes, leftover recovery, frontmatter, editor and
  pasted images.
- M2.1 — Editor polish: save robustness, live formatting, editor chrome and one Markdown parser for
  both views; see `docs/decisions/0007-one-markdown-parser.md`.
- M3 — Find and pin: content search, unified bar, pinned cards and note rows, in-place editing, focused
  panel and list keyboard; see "Note list" in `docs/DESIGN.md` and `docs/decisions/0005-pinned-items.md`.
- M4 — Web pages: mobile browser tab with history and suggestions, page selection read from Rust,
  pages saved as notes, code colors and Edge features filtered; see "Untrusted content" in
  `docs/ARCHITECTURE.md`.

## M4.1 — Preferences

Decided 2026-11-06: a third panel view opened by a gear icon replacing the folder icon, sections
General, Appearance, Browser and Shortcuts; settings stay in the existing atomic `settings.json`.

- [x] (standard) Preferences view; move the folder choice and panel side there (the tray keeps the
  side, in sync); returning to Notes after opening a page leaves the search field empty
- [x] (advanced) Resizable panel width (inner edge, min 320, max ~50% of the monitor), saved
- [ ] (standard) Note order: name, modified or created, ascending or descending, saved; drag to
  reorder pinned items (moves lines in `_pinned.md`); manual order only for pinned items
- [ ] (standard) Optional grouping by date when ordering by date

## M4.2 — Editor

- [ ] (standard) `@codemirror/search`: Ctrl+F in the note, Ctrl+D next occurrence, Alt+click cursors
- [ ] (standard) Floating formatting bar on selection, with shortcuts in tooltips
- [ ] (standard) Shortcut sheet on Ctrl+/, also listed in Preferences

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
