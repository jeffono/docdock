# 0006 — Web search suggestions

**Context:** the unified bar is used to search notes, create notes and open pages. Search engine suggestions
improve navigation, but sending every keystroke would send private note searches outside the computer.

**Decision:**

- **Level 1, enabled by default:** query the search engine only with clear web intent: in browser mode, or
  in notes after choosing "Search the web" or using the `?` prefix.
- **Level 2, opt-in:** Preferences > General can also enable suggestions while typing in the note
  search bar. Requests wait 300 ms and require at least three characters of free text. Note filters
  stay local, and requests pause during note editing. With the setting off, ordinary note searches
  stay on the computer.
- **Search engine:** Preferences offers DuckDuckGo (default), Brave, Startpage, Ecosia, Google, Bing
  and a custom HTTP(S) URL with `%s`. Presets use anonymous suggestion endpoints shipped by Firefox
  or documented by the provider when available. Custom URLs have no suggestions.
- **Request through Rust:** no cookies or referer. The response is untrusted JSON, displayed as text,
  never as HTML. The `main` window's CSP does not change.
- **Address history:** local, in `app_config_dir`, never in the notes folder.

**Consequences:**

- A lightweight HTTP client is added to Rust.
- The search engine receives the text and IP; offline or on timeout, only local suggestions appear.
- Late responses are discarded to avoid overwriting current input.
