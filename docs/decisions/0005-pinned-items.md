# 0005 — Pinned items in `_pinned.md`

**Context:** the panel shows pinned notes and web pages at the top, in an order chosen by the user.
The list must follow the folder between computers, work outside the app and avoid touching notes on
every click.

**Decision:**

- **File:** `_pinned.md` at the root of the notes folder. The `_` keeps it at the top of Explorer, and it
  appears in the app list like any note.
- **Format:** ordinary markdown. Each list item with a link is a pinned item, in file order: a relative
  link to a note, with spaces as `%20`, or an http(s) URL for a page. No `[[wikilinks]]`, which only
  Obsidian understands.
- **Line editing:** pinning adds a line, unpinning removes that link's line and reordering moves item
  bodies among item line slots, leaving non-item lines and line terminators in place. The app never
  rewrites the entire file, so handwritten headings and comments remain.
- **Pin position:** a new line goes after the last item, or at the end of the file if there is none; it
  uses the file's first line terminator.
- **Derived state** lives inside the notes folder ([0009](0009-note-privacy.md)): which cards are
  collapsed to one line in `.docdock/collapsed.json`, and the last version read in
  `.docdock/pinned-copy.json`. If `_pinned.md` disappears, the app offers "Restore pinned items";
  pinning something without restoring creates the file again.

**Consequences:**

- A note renamed or moved outside the app breaks the link; the item appears as "note not found", with an
  option to remove it.
- External edits to `_pinned.md` arrive through the notes' external change handling.
- Reordering from a displayed list requires its last-read version. External changes reject the move
  with a warning instead of silently reapplying stale positional intent.
- The file is user content: the notes' atomic writes apply.
- Versions before 0009 kept the derived state in AppData; it migrates once the folder is available.
