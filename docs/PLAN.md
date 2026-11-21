# Plan

Each stage shows its model level (see `AGENTS.md`) and cost: S (up to a day), M (days), L (one to two
weeks), XL (several weeks, high risk). Detail a stage before starting it. Completed stages are in the
git history (`git log --merges`) and releases in the tags.

## In progress

## Next

M18 — Privacy deterrents, not encryption (advanced, M): Windows Hello to show notes after the panel
has been hidden for a while, and notes marked private in the frontmatter shown blurred until
revealed. Notes marked private do not float, and Windows Hello also hides floating windows
(`floating::hide_all` and `show_all` are ready).

## Later

In this order:

- M19 — Rich content: MathML equations with Temml (standard, S–M; KaTeX depends on inline styling,
  blocked by CSP), then Mermaid diagrams (advanced, M–L), which inject `<style>`, also blocked. A
  sandboxed iframe with its own CSP would render code fences without loosening `main`. It is the first
  natural extension point, so the plugin decision is taken here and recorded before any plugin work.
  In-process JavaScript (the Obsidian model) breaks the untrusted-content and privacy decisions;
  isolated iframes or workers with declared permissions, or WASM in Rust, cost L–XL plus a stable
  public API. Until then, build optional features as internal modules. Fitting candidates: code-fence
  renderers, task sync, dictation, extra search providers and text transforms; not windows, updates,
  privacy or spell checking.
- M20 — Tasks (advanced, XL): local Markdown tasks, gathered and filtered, then bidirectional Android
  sync with Markdown as the durable content. First candidate: Microsoft To Do (free Android app or
  Tasks.org), no server needed. Todoist's free plan caps projects, and every plan caps active tasks;
  Google Tasks' API lacks times, reminders and recurrence; CalDAV needs a server or hosted account.
  Plan stable task identity, conflicts, deletions and recurrence, and recheck API and plan limits
  before choosing.

Reassess later, without a stage:

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
