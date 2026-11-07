# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

- M4.2: folder search, renaming, automatic names and frontmatter done; next, folder prefixes and
  grouping. Folder moving remains deferred.

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
- M4.1 — Preferences: gear view, saved panel width, note order and date grouping, pinned reorder by
  drag or keyboard, and list polish; see "Preferences" and "Note list" in `docs/DESIGN.md`.

## M4.2 — Files

Decided 2026-11-07: renaming moves the file without overwriting and updates `_pinned.md`; links in
other notes are not rewritten. A tree view is out of scope.

- [x] (advanced) Rename a note from its card title (double click or F2)
- [x] (advanced) Automatic name for `Untitled N` notes when leaving them: frontmatter `title`, else the
  top H1, else the first words (about 60 characters)
- [x] (standard) Frontmatter: the title field writes the title the file name cannot hold; key and
  tag completion in the raw block; Ctrl+; adds a property
- [ ] (standard) Show each note's folder as a quiet prefix and match it in search; grouping by folder
  as an alternative to grouping by date
- [ ] (standard) Later: move to a folder through a picker that can create it; delete only empty folders
  or to the Recycle Bin

## M4.3 — Editor

- [ ] (standard) `@codemirror/search`: Ctrl+F in the note, Ctrl+D next occurrence, Alt+click cursors
- [ ] (standard) Floating formatting bar on selection, with shortcuts in tooltips; include "Add
  property" (Ctrl+;)
- [ ] (standard) Shortcut sheet on Ctrl+/, also listed in Preferences
- [ ] (standard) Close brackets and quotes as they are typed (`closeBrackets` from
  `@codemirror/autocomplete`)
- [ ] (standard) Tags that do not read as tags fail silently (`tags: a, b`, `tags: [Plano: Q3]`): fix
  them where the intent is unambiguous, such as on leaving the note, else mark the line quietly; never
  rewrite text the user is still typing
- [ ] (basic) Theme in Preferences > Appearance: system (default), light or dark

## M5 — Distribute

- [ ] (basic) README in English, with a link to the pt-BR version
- [ ] (standard) Installer, automatic updates and decision on code signing
- [ ] (advanced) Quote the autostart path: `auto-launch` writes `Run` without quotes

## Later

In no defined order:

- MathML equations with Temml; KaTeX depends on inline styling, blocked by CSP.
- Mermaid diagrams: require a decision on CSP, which blocks the `<style>` it injects.
- Slash commands in the editor, including "Add property" for the frontmatter.
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
