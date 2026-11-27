# Plan

Each stage shows its model level (see `AGENTS.md`) and cost: S (up to a day), M (days), L (one to two
weeks), XL (several weeks, high risk). Detail a stage before starting it. Completed stages are in the
git history (`git log --merges`) and releases in the tags.

## In progress

## Next

M21 — Rich content and plugin boundary (standard to advanced, M–L):

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

## Later

In this order:

- M22 — Tasks (advanced, M for local; L–XL if adding native sync): split into two layers, since
  Microsoft To Do retires on 2027-06-30 with no personal-account replacement. First, local Markdown
  tasks (Obsidian Tasks-compatible dates and recurrence), gathered, filtered, toggled and
  quick-added inside DocDock (already usable on Android via folder sync with Markor, Zettel Notes or
  Obsidian Mobile). Second, if native mobile reminders are still needed, add open-standard
  bidirectional sync — CalDAV (`VTODO`) or serverless DecSync + Tasks.org (F-Droid) over Syncthing —
  rather than a proprietary cloud API (likely as a plugin once M21's boundary exists).

Reassess later, without a stage:

- Optionally float the Scratchpad (standard, M), with its own window kind and save command, and an
  edit lease shared between the tab and the floating window.
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
