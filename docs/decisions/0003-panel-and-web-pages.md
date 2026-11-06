# 0003 — Tauri approved in M0, with web page in its own window

**Context:** M0 existed to prove the panel before building the rest. Focus, animation, two monitors and
web pages were tested in the real app, and the user approved the experience. Multiple WebViews in the
same window remain behind the `unstable` feature in Tauri 2.12.5, and an iframe is unsuitable because
many sites block it.

**Decision:**

- Continue with Tauri; Electron is no longer an active alternative.
- Display web pages in their own window, owned by the panel, without `unstable`.
- Move both windows in the same `DeferWindowPos` transaction.
- Treat the page as untrusted: outside any capability, http(s) only and no new windows.

**Consequences:**

- Remote-origin `invoke` is rejected by Tauri's ACL, but `__TAURI_INTERNALS__` exists on the page.
  Never grant this window a `remote` capability. Custom commands do not require a capability for local
  origins, so `lib.rs` accepts them only from the `main` window and the page does not navigate to app origins.
- The page shares the UI's WebView2 profile; only the rendering process is separate.
- `add_child` would require removing `WS_EX_NOACTIVATE` from the main window, so it was ruled out.
- The page does not talk to Rust: focus and Esc are observed by the panel thread.
- Rust may pull data from the page by running a constant script through WebView2 `ExecuteScript` and
  decoding its result as untrusted; the page still has no bridge, message or host object to call Rust.
- Without a web page, the hidden app uses about 160 MB of private memory; the page adds whatever the site
  requires (github.com exceeded 450 MB in development).
