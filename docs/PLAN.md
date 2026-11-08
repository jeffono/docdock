# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

- M4.4 — Move and delete, on branch `move-delete`.

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
- M4.2 — Files: renaming from the card title, automatic names, file names beside differing titles,
  frontmatter completion and Ctrl+;, folder prefixes and grouping; see "Note list" in `docs/DESIGN.md`.
- M4.3 — Editor: find and replace in the note, several cursors, formatting bar, shortcut sheet on
  Ctrl+/, bracket pairing, tag repair on leaving and theme choice; see the editor section of
  `docs/DESIGN.md`.

## M4.4 — Move and delete

Renaming already moves files without overwriting and updates `_pinned.md`; links in other notes are
not rewritten. A tree view is out of scope.

Decided 2026-11-07:
- Deleting asks first, in an in-panel dialog with focus on Cancel and "Move to Recycle Bin", then
  offers Undo in a notice. If restoring from the Recycle Bin is unreliable, Undo may keep a temporary
  copy until the panel closes.
- Ctrl+Delete deletes the selected card from the list or search results. In the editor and the search
  bar it keeps deleting the next word, and Delete alone still only removes missing pinned items. No
  delete button on the card header.
- A right-click menu on cards, opened inside the panel like the order menu (also Shift+F10 and the
  Menu key), holds Rename, Pin, Move and Delete, each showing its shortcut.
- Moving opens an in-panel picker: a text field filtering the existing folders, the root included,
  and "Create folder X" when the typed name does not exist.
- A moved note's relative links and images are rewritten to keep pointing at the same files; no
  file moves with it.
- Folder labels appear only for folders with notes (directly or below) or completely empty folders; attachment-only folders have no app deletion action, and confirmation counts notes and other files.
- Folders are deleted from a right-click menu on their group label (also Shift+F10 and the Menu key)
  when grouping by folder: an empty folder goes at once, one with content asks first and goes to the
  Recycle Bin. Folders left empty by moves or deletes are kept.

- [x] (advanced) Delete a note to the Recycle Bin from its card and the keyboard, with undo while the
  panel is open; pinned entries follow, and an open editor stops saving instead of recreating it
- [x] (advanced) Move a note to a folder through a picker that can create it, reusing rename's
  no-overwrite move and pin updates
- [x] (standard) Delete empty folders, or folders to the Recycle Bin after confirmation

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
