# 0001 — Tauri 2 with Svelte 5 and Vite

**Context:** the app needs to display full markdown and web pages, access Win32 APIs easily and, in the
future, run local voice models. The development cycle needs to be fast.

**Decision:** use Tauri 2 (Rust) with WebView2 and a Svelte 5 UI with Vite, without SvelteKit: a panel
without routes does not need an application framework.

**Consequences:**

- Small installer and direct access to Win32 and local AI Rust libraries.
- Memory use is close to other Chromium apps; reducing it depends on measures such as lowering WebView2
  memory priority while the panel is hidden.
- The web engine varies across systems, and Linux will be the most labor-intensive platform.
- If M0 validation fails, the alternative is Electron. The UI can be reused.
