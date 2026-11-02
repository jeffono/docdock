# 0004 — Untrusted content in the main window

**Context:** M1 will render markdown in the UI itself, which talks to Rust. A note may come from third
parties through synchronization or download. HTML injected into the `main` window would reach every
command, and opening a note must not trigger network access. In Tauri 2.12.5, the asset protocol has broad
scope, CSP does not prevent navigation and `tauri.conf.json` offers no per-window navigation control.

**Decision:**

- **Markdown:** converted in Rust with `pulldown-cmark` and sanitized with `ammonia` using an allowlist.
  The UI receives only sanitized HTML, and this is the only permitted use of `{@html}`. Raw text never
  goes into the DOM as HTML.
- **Local images:** custom `note-asset` protocol instead of the asset protocol. It serves only png, jpg,
  jpeg, gif and webp inside the notes folder, with the guarantees of `resolve`. The extension applies to
  both the request and the canonical target. SVG is excluded because it can contain scripts. The type
  comes from the extension, with `nosniff` and `no-store`.
- **Only the `main` window reads images:** the protocol is registered in all WebViews, including the `web`
  window, and the `lib.rs` barrier covers only commands. The request carries the WebView label, and any
  other gets 403. Without this, a site could learn which images exist and display them.
- **Remote images:** blocked by CSP for privacy.
- **CSP:** `default-src 'none'`, allowing only `'self'` for scripts, styles and images, `note-asset` and
  IPC at `http://ipc.localhost`. No `'unsafe-inline'`, `dangerousDisableAssetCspModification` or `devCsp`.
- **Navigation:** `main` is created in `main_window.rs` with `create: false`, because only the builder
  accepts per-window rules. It navigates only within the UI origin, `http://tauri.localhost` in the
  production build and `devUrl` in dev, and never opens another window.
- **`Object.freeze(Object.prototype)`:** injected only in `main`. Tauri's `freezePrototype` runs in every
  WebView and would break sites in the `web` window.
- **Capabilities:** remain only `core:default`. The folder picker is called by Rust, and
  `tauri-plugin-dialog` commands are not granted to the UI.

**Consequences:**

- CSP exists only in the production build. In desktop dev, the page comes directly from Vite, without
  passing through the Tauri protocol that adds the header, and `devCsp` would have no effect. Test security
  and styling in the production build.
- The UI cannot use a `style` attribute in HTML, runtime `<style>`, `eval` or `data:`. Setting styles
  through CSSOM works, and Svelte CSS is emitted as a file, so there is no `<style>` nonce.
- The UI's `<form>` must call `preventDefault`: `form-action 'none'` blocks actual submission.
- CSP covers subframes and `on_navigation` covers the main frame. This control fails open for a URL the
  `url` crate cannot parse, which Chromium should not produce.
- Links in rendered content do not navigate. To open them in the default browser, the UI intercepts the
  click and calls a command that validates http(s) and mailto. Never relax `on_navigation` or
  `on_new_window` for this.
- The `note-asset` path is relative to the notes folder, with percent-encoding. The renderer rewrites
  `src` to `http://note-asset.localhost/<path>`.
- Accepted risk: swapping a folder in the notes tree for a junction between `resolve` and the write can
  redirect a save; it needs concurrent local write access to the tree and a target holding the same bytes
  as the base version. Handle-based validation is deferred.
