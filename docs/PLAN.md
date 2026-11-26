# Plan

Each stage shows its model level (see `AGENTS.md`) and cost: S (up to a day), M (days), L (one to two
weeks), XL (several weeks, high risk). Detail a stage before starting it. Completed stages are in the
git history (`git log --merges`) and releases in the tags.

## In progress

M19 — Clipboard manager and text transforms (`clipboard` branch).

## Next

M19 — Clipboard manager and text transforms (advanced, M): a top-bar tab mirroring Windows
Clipboard History (`Win+V`) through WinRT `Clipboard::GetHistoryItemsAsync` (start with a short
spike confirming unpackaged behavior and focus rules; when history is off, show the current item
with a notice and link to Windows Settings). Expands the shared pure transforms (`transforms.ts`,
used across Clipboard, Scratchpad and notes): digits only, strip punctuation/whitespace
(alphanumeric), normalize whitespace/line breaks, strip accents, remove empty lines, join lines,
clean tracking parameters from URLs, URL decode and format/minify JSON, with contextual top
suggestions, chaining/preview, and transformed copies written out of history so `Win+V` stays
clean. Includes bar search, counts, deleting history items, Send to Scratchpad and Create note.

## Later

In this order:

- M20 — Editable floating notes and title-bar collapse (advanced, M–L): adds a quiet title bar to
  post-its (keeping floating images frameless) as a drag handle and a collapse/expand ("window
  shade") toggle, remembering collapsed state and expanded height in `floating.json` with vertical
  resize locked while collapsed. Post-its remain read-only and non-activating by default;
  double-click or the pencil enters inline editing using M18's decoupled editor with on-demand
  activation (`Esc` returns to read-only), plus an action to open in the panel. Extends `0014` with
  `float_save_note` scoped strictly to the window's registered path and folder, coordinates editing
  so a note edits in one place at a time, and flushes editing post-its in the exit gate. Optionally
  allows floating the Scratchpad.
- M21 — Rich content and plugin boundary (standard to advanced, M–L):
  1. Low-cost safe Markdown and local media (standard, S–M): clickable task checkboxes in reading
     view, `==highlight==`, collapsible callouts (`<details>`), local SVG via `<img>` with a
     `sandbox` CSP on `note-asset`, local audio/video (HTTP `Range` in `note-asset`, `media-src`,
     unused-attachment tracking), local CSV tables, MathML equations with Temml, and PDF embeds
     (system-open card first, or on-demand `pdf.js` canvas preview).
  2. Sandboxed renderers and incremental plugin model (advanced, M–L): Mermaid diagrams inside a
     sandboxed iframe with its own CSP without loosening `main`. Record the plugin decision before
     plugin work: lock only the two hard-to-reverse invariants — never in-process JS in `main`
     (protecting `0004` and `0009`), and all sensitive effects (allowlisted network, note access,
     local CLI execution) mediated by Rust through explicit manifest permissions. Start with an
     experimental `v0` contract and add extension points only as needed: code-fence renderers
     (Mermaid), text actions (translation, AI writing assistance) and isolated widgets/tabs (stock
     tickers, AI usage monitors, task sync connectors).
- M22 — Tasks (advanced, M for local; L–XL if adding native sync): split into two layers, since
  Microsoft To Do retires on 2027-06-30 with no personal-account replacement. First, local Markdown
  tasks (Obsidian Tasks-compatible dates and recurrence), gathered, filtered, toggled and
  quick-added inside DocDock (already usable on Android via folder sync with Markor, Zettel Notes or
  Obsidian Mobile). Second, if native mobile reminders are still needed, add open-standard
  bidirectional sync — CalDAV (`VTODO`) or serverless DecSync + Tasks.org (F-Droid) over Syncthing —
  rather than a proprietary cloud API (likely as a plugin once M21's boundary exists).

Reassess later, without a stage:

- Privacy deterrents (former M18): Windows Hello lock after inactivity (advanced, M; high friction
  against the quick-reference hover flow, and Cryptomator already covers real encryption and
  biometrics; `floating::hide_all` and `show_all` remain in place if revisited) and optional
  `private: true` frontmatter blur (standard, S; opt-in shoulder-surfing deterrent).
- Dictation with local models, downloaded separately from the installer (advanced, L–XL). First test
  whether Windows voice typing (Win+H) in the editor covers the need.
- Code signing before 1.0 (see `docs/decisions/0008-distribution.md`). Free: SignPath Foundation once
  the project has verifiable reputation (it appears as publisher), or the Microsoft Store with a free
  individual account, which signs only MSIX. Tauri does not build MSIX, so the Store means a second
  channel without our updater, autostart through a package `StartupTask`, offline WebView2 and testing
  the panel packaged; a paid Store edition is a possible revenue model while the code stays MIT. Paid:
  Azure Artifact Signing (about US$10/month) or an OV certificate (about US$200–400/year).
- macOS and Linux: a new `platform/` implementation. On Wayland, the panel depends on layer-shell,
  which GNOME does not provide.
