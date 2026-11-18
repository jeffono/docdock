# 0001 — Tauri 2 with Svelte 5 and Vite

**Context:** the app needs to display full markdown and web pages, access Win32 APIs easily and, in the
future, run local voice models. The development cycle needs to be fast.

**Decision:** use Tauri 2 (Rust) with WebView2 and a Svelte 5 UI with Vite, without SvelteKit: a panel
without routes does not need an application framework.

**Consequences:**

- Small installer and direct access to Win32 and local AI Rust libraries.
- Memory use is close to other Chromium apps; WebView2's memory target is lowered while the panel is
  parked (see "Memory while parked" in `docs/ARCHITECTURE.md`).
- The web engine varies across systems, and Linux will be the most labor-intensive platform.
- Electron was the fallback if M0 failed; M0 approved Tauri ([0003](0003-panel-and-web-pages.md)).
