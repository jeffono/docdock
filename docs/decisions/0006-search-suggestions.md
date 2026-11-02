# 0006 — Web search suggestions

**Context:** the unified bar is used to search notes, create notes and open pages. Search engine suggestions
improve navigation, but sending every keystroke would send private note searches outside the computer.

**Decision:**

- **Level 1, enabled by default:** query the search engine only with clear web intent: in browser mode, or
  in notes after choosing "Search the web" or using the `?` prefix. Text typed to search notes never leaves
  the computer.
- **Level 2, option disabled by default:** query on every keystroke also when searching notes, with a short
  delay and minimum number of letters.
- **Search engine:** DuckDuckGo by default, changeable in settings. Google has no official suggestions URL.
- **Request through Rust:** no cookies or referer. The response is untrusted JSON, displayed as text,
  never as HTML. The `main` window's CSP does not change.
- **Address history:** local, in `app_config_dir`, never in the notes folder.

**Consequences:**

- A lightweight HTTP client is added to Rust.
- The search engine receives the text and IP; offline or on timeout, only local suggestions appear.
- Late responses are discarded to avoid overwriting current input.
