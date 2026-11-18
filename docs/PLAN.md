# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

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
- M9 — Clean pages: balanced tracking prevention on the page profile, content blocking with Brave's
  `adblock` crate from downloaded lists with cosmetic hiding and no scriptlets (about 8 MB), a shield to
  allow a site, and a reader view that extracts the article with `dom_smoothie` and saves it as a note;
  see `docs/decisions/0010-content-blocking.md`, `docs/decisions/0011-reader-view.md` and "Untrusted
  content" in `docs/ARCHITECTURE.md`. The page window reaches no command from any origin and keeps
  native dialogs (`rfd` replaced `tauri-plugin-dialog`). Site permissions are asked in the panel and
  remembered per site; see `docs/decisions/0012-site-permissions.md`. Rust tests grew 23%, on site
  matching, the engine cache, reader bounds, early hiding state, page IPC and permission requests.
- Release 0.2.0: first SemVer version; automatic update from 0.1.0-beta.3 validated.
- M10 — Identity: hand-drawn app icon with a rebuild script, monochrome tray glyphs that follow the
  taskbar's theme and scale, an edge handle (a 300% activation stretch, a 400% reveal area, a rush
  that opens at once) whose tab is drawn by the panel's own transparent WebView, and a debug-only
  tweak window for tokens, slides, the handle and the tab's shape with presets and exported diffs;
  see "Motion" in `docs/DESIGN.md` and "Trigger" and "Expansion" in `docs/ARCHITECTURE.md`.
  The native tab (`tab.rs`, `tiny-skia`) was replaced after a spike. Rust tests grew 12% and frontend
  tests 18%, mostly the tweak window's export, presets and validation and the handle's rules.
- Release 0.3.0: automatic update from 0.2.0 validated.
- Audit: docs without contradictions or verification narratives, duplicated code consolidated,
  dev-only and redundant tests removed, `main`'s capability narrowed to events, and unreadable
  subfolders skipped with a notice that folds into the search bar; see "Notes" in
  `docs/ARCHITECTURE.md`.

## Next

M11 — Resident app health:

- [ ] Updates in Preferences (standard): an About/Updates section with the current version, a
  "Check now" button and the state (up to date, downloading, ready to restart). Checks stay in
  Rust (`updates.rs`), reached through new `ipc.ts` commands; webviews keep no updater
  permission. Check at startup, every 6 hours and after resume from sleep. A found update
  downloads and is announced without disruption: no blocking dialog, no focus stolen; restart
  stays a user action through the existing exit gate.
- [ ] Lower memory while hidden (advanced): reduce WebView2 memory use (memory target or priority)
  while the panel is hidden; see decision 0001. Measure before and after.

## Later

In this order; each line is a future stage. Estimated cost: S (up to a day), M (days), L (one to two
weeks), XL (several weeks, high risk).

- M12 — Editor productivity:
  - Discreet character and word count (basic, S): `Intl.Segmenter` for words, selection count when
    there is one, frontmatter excluded.
  - Text transforms (standard, S–M): UPPER, lower, Sentence case and Capitalize Each Word with the
    UI locale; maybe sort and deduplicate lines. Reached from the formatting bar, shortcuts and slash
    commands.
  - System spell checker, on by default with a Preferences toggle (advanced, spike first): WebView2
    uses the Windows checker with Windows' languages; a CodeMirror 6 report says Chromium 152 never
    underlines its content ([forum](https://discuss.codemirror.net/t/native-spellcheck-is-never-applied-to-a-codemirror-6-editing-host-on-chromium-152-electron-44/9886)).
    If it works: no checking in code, URLs and frontmatter, and keep the spelling items in the
    context menu filter of `platform/edge.rs`.
  - Slash commands, including "Add property" for the frontmatter (standard, M). Text transforms and
    slash commands are the two uses that justify one list of editor commands.
- M13 — Search (standard, S–M): choice of search providers (presets such as DuckDuckGo, Brave,
  Startpage, Ecosia, Google and Bing, plus a custom `%s` URL without suggestions; suggestions only where
  an endpoint is stable) and web suggestions while searching notes (level 2: an option, off by default,
  to query on every keystroke after a short delay and a minimum length). Amends decision 0006.
- M14 — Capture (advanced, M): screenshots of a region anywhere on the screen and of the web tab,
  inserted through the pasted-image path.
- M15 — Privacy deterrents, not encryption (advanced, M): Windows Hello to show notes after the panel
  has been hidden for a while, and notes marked private in the frontmatter shown blurred until
  revealed.
- M16 — Rich content: MathML equations with Temml (standard, S–M; KaTeX depends on inline styling,
  blocked by CSP), then Mermaid diagrams (advanced, M–L), which inject `<style>` blocked by CSP. A
  sandboxed iframe with its own CSP would render code fences without loosening `main`; it is also the
  first natural extension point, so the plugin decision is taken here.
- M17 — Post-its as floating desktop windows (advanced, L).
- M18 — Tasks (advanced, XL): first local Markdown tasks, gathered and filtered, then bidirectional
  Android sync, keeping Markdown as durable local content.
  First candidate: Microsoft To Do as the service, with its free Android app or Tasks.org; no
  self-hosted server required. Todoist is an alternative with a sync-oriented API, but its free plan
  allows 5 personal projects and all plans cap active tasks at 300 per project; custom reminders
  require Pro. Google Tasks is less suitable: its API omits scheduled times, reminders and recurrence.
  CalDAV remains an open alternative but requires a server or hosted account. Before choosing, plan
  stable task identity, conflict handling, deletions and recurrence without losing Markdown content;
  validate ID changes, completion and incremental sync against the chosen service. Recheck current
  API and plan limits during planning; see [Graph tasks](https://learn.microsoft.com/en-us/graph/api/resources/todotask),
  [Tasks.org sync](https://tasks.org/docs/sync_microsoft/) and [Todoist limits](https://www.todoist.com/help/account-and-billing/plans/usage-limits-in-todoist-e5rcSY).

Reassess later, without a stage:

- Plugins: deferred until M16; require a recorded decision before starting. In-process JavaScript
  (the Obsidian model) breaks the untrusted-content and privacy decisions; isolated iframes or
  workers with declared permissions, or WASM in Rust, cost L–XL plus a stable public API. Build
  optional features as internal modules first. Fitting candidates: code-fence renderers (Mermaid,
  math), task sync, dictation, extra search providers and text transforms; not windows, updates,
  privacy or spell checking.
- Dictation with local models, downloaded separately from the installer (advanced, L–XL). First test
  whether Windows voice typing (Win+H) works in the editor; it may cover the need.
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
| Handle position            | Vertically centered on the work area; not movable                           |
