# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

- M9 — Clean pages: tracking prevention, content blocking and reader view on branches `m9-tracking`,
  `m9-adblock` and `m9-reader`, merged into `m9`.

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
- M7 — Versatile search: optional relative dates, `#tag` and `date:` filters with completion, tags
  seeded on creation and web actions as a list section; see "Note list" in `docs/DESIGN.md` and
  "Notes" in `docs/ARCHITECTURE.md`. Frontend tests grew 32%, mostly pure parser and date-range cases.
- M8 — Note privacy: decision 0009, vault-friendly unavailable folder with a `.docdock/folder-id`
  marker, derived data inside the notes folder, a 30-day trash inside the folder where the Recycle Bin
  refuses (Preferences > Trash), deletes without confirmation and an Undo notice that draws attention,
  capture exclusion, private clipboard and an Unlock action for a Cryptomator vault file; see "Notes" in
  `docs/ARCHITECTURE.md`. Rust tests grew 33%, on content loss and link safety around the folder marker,
  the unavailable folder and the trash.
- Release 0.1.0-beta.3: automatic update from 0.1.0-beta.2 validated.

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
