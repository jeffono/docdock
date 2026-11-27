# Plan

Each stage shows its model level (see `AGENTS.md`) and cost: S (up to a day), M (days), L (one to two
weeks), XL (several weeks, high risk). Detail a stage before starting it. Completed stages are in the
git history (`git log --merges`) and releases in the tags.

## In progress

- M20 step 1 (collapsed state in `floats.rs`).

## Next

M20 — Editable floating notes and title-bar collapse (advanced, M–L). Branch `floating-notes-edit`.
Decided with the user: the post-it bar shows the note's title and takes the pill's controls (images
stay frameless); a double click on the bar or its chevron collapses/expands; asking to edit a note
already edited elsewhere moves the edit there (the other place saves and returns to reading);
floating the Scratchpad is deferred. Steps, one commit each:

1. `floats.rs`: optional `collapsed` in `floating.json` entries, no version bump (`height` stays the
   expanded height); pure collapsed/expand/fit rects and a vertical lock in `constrain_free`.
2. `floating.rs`: `float_set_collapsed`, sizing hook lock, saving the expanded height, restore.
3. UI: post-it title bar (title, chevron, controls), edges limited while collapsed. Screenshot for
   approval.
4. Rust edit lease in the floating registry (one editor per note across panel and post-its),
   `float_begin_edit`/`float_end_edit`, on-demand activation (`WS_EX_NOACTIVATE` cleared while
   editing, foreground given back on leaving), `dock::track_focus` ignoring floating windows.
5. `float_save_note` with no path argument: the label's registered path read under `SAVING`, in the
   folder identity it opened on, with base-version conflicts and `ReplaceFileW`.
6. Vault lock or folder change parks editing post-its (content hidden, text kept) instead of
   destroying them; the returning folder adopts them.
7. Finish handshake (fail-closed with timeout) for exit gate, folder switch, closes and edit
   transfer between panel and post-its.
8. Post-it inline editing with `MarkdownEditor` (lazy, no spelling, private copy via a write-only
   secondary command), pure edit-state machine with tests; Esc leaves, focus loss saves.
9. Notes.svelte coordination (claim, transfer, "Editing in a floating window") and "Open in panel".
10. Docs (`0014` extended, ARCHITECTURE, DESIGN), test review, cross-provider review, CI.

## Later

In this order:

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
