# 0005 — Pinned items in `_pinned.md`

**Context:** the panel will show pinned notes and web pages at the top, in an order chosen by the user.
The list must follow the folder between computers, work outside the app and avoid touching notes on every click.

**Decision:**

- **File:** `_pinned.md` at the root of the notes folder. The `_` keeps it at the top of Explorer, and it
  appears in the app list like any note.
- **Format:** ordinary markdown. Each list item with a link is a pinned item, in file order: a relative
  link to a note, with spaces as `%20`, or an http(s) URL for a page. No `[[wikilinks]]`, which only
  Obsidian understands.
- **Line editing:** pinning adds a line, unpinning removes that link's line and reordering moves item bodies among item line slots, leaving non-item lines and line terminators in place.
  The app never rewrites the entire file, so handwritten headings and comments remain.
- **Pin position:** a new line goes after the last item, or at the end of the file if there is none; it
  uses the file's first line terminator.
- **Collapsed state:** which cards are collapsed to one line is stored in `.docdock/collapsed.json` inside the notes folder.
- **Restore:** the app stores the last version read in `.docdock/pinned-copy.json` inside the notes folder. If `_pinned.md` disappears, it
  offers "Restore pinned items"; pinning something without restoring creates the file again.

**Consequences:**

- A note renamed or moved outside the app breaks the link; the item appears as "note not found", with an
  option to remove it.
- External edits to `_pinned.md` arrive through M2's external change handling.
- Reordering from a displayed list requires its last-read version. External changes reject the move
  with a warning instead of silently reapplying stale positional intent.
- The file is user content: M2's atomic writes apply.

M8 privacy amendment: derived state moved inside the notes folder to follow decision 0009. Legacy
AppData state migrates after the folder becomes available; the markdown format is unchanged.
