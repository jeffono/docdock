# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

- M7: tasks merged into the `m7` integration branch; awaiting manual verification and merge.

## M7 — Versatile search

- [x] Relative dates ("2 hours ago", "yesterday") through `Intl.RelativeTimeFormat`, with the full date on
  hover, as an option in Preferences. `basic`
- [x] `#tag` filters with completion from the folder's tags; several tags all apply, combined with free
  text. Query parsing is a pure function. `standard`
- [x] `date:` filters: completion offers presets (today, last 7 days, this month, last month) that insert
  tokens such as `date:7d`; typed ISO dates and ranges (`date:2026-10..2026-11`) also work. The date is
  the frontmatter `created` or `date` when present, else the modified time. No calendar or chips yet.
  `standard`

## M8 — Note privacy

Threats that matter: leaks through cloud sync and a stolen device with synced notes; screen sharing
also matters for a panel that appears over other apps. Notes stay plain `.md`; encryption is delegated
to a vault the user mounts, such as Cryptomator, which stays optional. The app's own encryption is in
`docs/EXPLORATIONS.md`.

- [ ] Record the decision above in `docs/decisions/`, including the rule that nothing derived from note
  content is stored outside the notes folder. `basic`
- [ ] Unavailable notes folder (locked vault, unplugged drive, offline share): a state distinct from a
  deleted folder; never create the notes folder or its parents, which would write plaintext under a
  vault's mount point (regression test); a failed save keeps the text in memory with a notice and
  writes nowhere else, then saves when the folder returns; note text and the search index are
  cleared from memory while the folder is unavailable. `advanced`
- [ ] Nothing derived outside the notes folder: `pinned-copy.json` and the collapsed pinned paths in
  `settings.json` hold note content and names; check whether the WebView2 cache keeps note images.
  `standard`
- [ ] Deleting on volumes without a Recycle Bin, such as a mounted Cryptomator vault: verify, then give
  such notes an Undo that keeps the copy inside the notes folder. `advanced`
- [ ] Hide the panel from screen capture and sharing with `WDA_EXCLUDEFROMCAPTURE`. `standard`
- [ ] Copying from the editor stays out of clipboard history and cloud clipboard. `advanced`
- [ ] Optional vault file in Preferences: an Unlock action on the unavailable state opens
  `vault.cryptomator`, which Cryptomator unlocks with Windows Hello. `standard`
- [ ] README section in English and pt-BR: protecting notes with Cryptomator, including that moving notes
  into a vault does not remove earlier plaintext from cloud history. `basic`

## M9 — Clean pages

Space in the panel is small, so page clutter costs more than in a browser. These tasks need review by
another provider: they touch untrusted content.

- [ ] Strict tracking prevention on the page profile (`PreferredTrackingPreventionLevel`). `standard`
- [ ] Content blocking with Brave's `adblock` crate (MPL-2.0): network rules through
  `WebResourceRequested` and cosmetic rules hiding elements, which matter most in a narrow panel (cookie
  banners, sticky bars). Lists downloaded and updated, not bundled; scriptlets off, since they run code in
  the page; measure memory. Start with domain rules if the engine proves costly. `advanced`
- [ ] Reader view: read the loaded page through CDP, extract the article in Rust with a Readability
  port, convert it to Markdown and show it in the existing renderer; "Save page as note" can then save
  the whole article. Text only at first if CSP blocks remote images. `advanced`

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
- M4.4 — Move and delete: card menu, notes to the Recycle Bin with undo while the panel is open,
  moving through a folder picker with relative links rewritten, and folder deletion from group labels;
  see "Note list" in `docs/DESIGN.md` and "Notes" in `docs/ARCHITECTURE.md`.
- M5 — Distribute: README in English and pt-BR, MIT license, per-user NSIS installer, signed automatic
  updates from GitHub Releases, release workflow and quoted autostart path; see
  `docs/decisions/0008-distribution.md`.
- M6 — Panel adjustments: search and expanded note persist, pinned panel releases focus, Ctrl+, and
  Ctrl+Shift+D, Start with Windows and a custom activation shortcut; see "Note list", "Preferences"
  and "Panel navigation" in `docs/DESIGN.md`.
- Release 0.1.0-beta.2: automatic update from 0.1.0-beta.1 validated.

## Later

In no defined order:

- MathML equations with Temml; KaTeX depends on inline styling, blocked by CSP.
- Mermaid diagrams: require a decision on CSP, which blocks the `<style>` it injects.
- Slash commands in the editor, including "Add property" for the frontmatter.
- Post-its as floating desktop windows.
- Screenshots.
- Dictation with local models, downloaded separately from the installer.
- Plugins: require a recorded decision before starting.
- Privacy deterrents, not encryption: Windows Hello to show notes after the panel has been hidden for a
  while, and notes marked private in the frontmatter shown blurred until revealed.
- Code signing, free options (see `docs/decisions/0008-distribution.md`): SignPath Foundation once the
  project has verifiable reputation, which shows SignPath Foundation as publisher; or the Microsoft
  Store with a free individual account, which signs only MSIX packages. Tauri does not build MSIX, so
  the Store means a second channel without our updater, autostart through a package `StartupTask`
  instead of `Run`, offline WebView2 and testing the panel packaged. A paid Store edition for
  convenience is a possible revenue model while the code stays MIT.
- macOS and Linux: new `platform/` implementation. On Wayland, the panel depends on layer-shell, which
  GNOME does not provide.

## Open questions

Defaults used until you decide otherwise.

| Question                   | Default                                                                     |
| -------------------------- | --------------------------------------------------------------------------- |
| Monitor                    | The outermost on the chosen side, whose edge holds the cursor               |
| Close when mouse leaves    | Yes, except when pinned, actively editing or the page has focus             |
| Page over full-screen apps | Works in windowed and borderless full-screen; exclusive-mode games excluded |
| Panel in screen captures   | Always hidden, which also blanks the user's own screenshots of the panel    |
| Clipboard from notes       | Kept out of history and cloud clipboard; not cleared after a delay          |
