# Plan

Each task indicates the model level (see `AGENTS.md`). Mark `[x]` in the commit that completes it and
detail each stage when it is next.

## In progress

- M10 integrated on local branch `m10` (worktree `../docdock-wt/m10`; `tweak-polish` and
  `handle-reveal` merged, batched placement and capture-excluded tab added). The user checked it
  manually; the tab moves into the panel's WebView next (see the task below), then the look chosen
  with the tweak window goes to the defaults. Native page bar offset still ignores `--size-bar`.
- Spike `tab-spike` (worktree `~/.t3/worktrees/docdock/t3-b05596e9-03bccc18f7a51521`, head
  `412a3ef`, from `m10`) passed the user's manual check: no resize flicker, clicks pass through,
  focus and activation unchanged. Throwaway: production is rebuilt on `m10`, using it as reference.

## Next

M10 — Identity: official icon, a dev-only tweak panel to explore the look in the real app, and an edge
handle that opens the panel.

- [x] App icon (`standard`). Hand-drawn SVGs per size, from the user's files, go to
  `src-tauri/icons/source/` with a script that rebuilds everything derived. `icon.ico` takes the
  drawn 16, 20, 24, 30, 32, 48 and 256 and renders 36, 40 and 64 from the 480 master; the PNGs in
  `tauri.conf.json` are regenerated. Only the user changes the drawings.
- [x] Tray icon (`advanced`). Monochrome glyphs (black for a light taskbar, white for a dark one) at the
  size for the monitor's scale (16, 20, 24, 32, 48; 40 and 64 rendered), switched when the
  taskbar theme changes. The taskbar theme (`SystemUsesLightTheme`) is separate from the apps theme.
  Today `tray.rs` uses the colored window icon. Manual check at 100%, 125% and 150%, light and dark.
- [ ] Tweak panel (`standard`; keeping the panel open is `advanced`). Development builds only, loaded
  behind `import.meta.env.DEV` and absent from the release bundle. Edits `tokens.css` variables
  live through CSSOM, which CSP allows: both `light-dark` values of each color with the WCAG
  contrast shown for text and background pairs, system fonts (including Segoe UI Variable),
  font sizes, spacing, radius and control sizes. Named presets switch A/B, and an export gives the
  `tokens.css` diff to apply. A separate window tweaks while the panel behaves as in production. A chosen look goes to
  `tokens.css` and `docs/DESIGN.md` with a screenshot for approval.
- [x] Edge handle (`advanced`). Only a short stretch of the edge around a handle opens the panel,
  instead of the whole column, to avoid accidental openings. The handle appears only while the
  cursor is near. A window at the edge must not take clicks from scrollbars or maximized window
  buttons, so it stays click-through and is shown only on proximity. When the panel is open, the
  handle becomes a tab outside its inner edge with the app identity (vertical "DocDock" or the
  icon). The trigger rule lives in pure functions with tests; visual behavior gets a manual
  verification script. See the defaults in "Open questions".
- [ ] Tab in the panel's WebView (`advanced`). The native tab window imitated the panel with a second
  renderer: its border never matched exactly at 125%/150%, it trailed the panel by a frame during
  resize, and every panel rule (capture exclusion) had to be copied. Future tab content (voice
  input, AI token usage, GPU/RAM/network meters) is easier in HTML. Decided after `tab-spike`:
  - The main window is transparent and wider by the tab depth on the inner side; one SVG path draws
    the inner-edge line or focus band down the panel and around the tab, so there is no seam.
    Remaining in the spike: a subtle 1 px line where the tab meets the panel.
  - Clicks: `SetWindowRgn` shaped row by row to panel plus tab outline (about 1 px beyond the curves),
    combined with the monitor clip; while parked and still, `WS_EX_LAYERED | WS_EX_TRANSPARENT`.
  - Parked: the tab's reveal shift pushes the whole panel, so only the tab shows; reveal, rush and
    activation rules in `handle.rs` are unchanged.
  - Remove `tab.rs`, `platform/handle.rs`, `tiny-skia`, `handle-look.ts`, `panel_handle_look` and
    `Look`. Rust sends the tab shape to the UI instead of duplicating constants.
  - Fix what the wider viewport shifts: menus, dialogs and the width in Preferences position against
    the panel, not the viewport. The tab click keeps the native drag tolerance and release-on-tab rule.
  - Risks to check: a flash when toggling `WS_EX_LAYERED`, dark `color-scheme` painting the
    transparent area, Windows 10. Measured with the panel parked: RAM and GPU equal to the native tab
    within 5 MB.

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
| Handle position            | Vertically centered on the work area; not movable                           |
| Handle activation stretch  | 300% of the tab height, centered on it and clamped to the work area         |
| Handle reveal area         | Within 400% of the tab height around the tab; hides with the panel          |
| Rush to the edge           | From 3000 logical px/s toward the edge: opens at once, no handle slide-in   |
| Clicking the open tab      | Closes the panel; dragging it does nothing                                  |
