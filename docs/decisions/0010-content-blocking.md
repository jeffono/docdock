# 0010 — Content blocking with downloaded filter lists

**Context:** the page window is a narrow panel, so ads, sticky bars and cookie banners cost more space
than in a browser. Blocking well needs a maintained engine and maintained lists; writing either is out
of scope. The page window must keep having nothing it can call ([0004](0004-untrusted-content.md)),
and the app's UI must never be filtered.

**Decision:**

- **Engine:** Brave's `adblock` crate (MPL-2.0), used unmodified as a dependency, without its
  `single-thread` feature so the engine is `Send + Sync`: built off the UI thread, matched on it.
  Measured on EasyList + EasyPrivacy + the cookie list: about 7 MB more working set once loaded from
  its cache, 10 MB during a build, 130 ms to build, 7 ms to load and 3 µs per request (release build).
  Domain-only rules were not needed.
- **Lists:** EasyList, EasyPrivacy and the EasyList Cookie List
  (`secure.fanboy.co.nz/fanboy-cookiemonster.txt`, plain Adblock Plus syntax), downloaded over HTTPS
  and never bundled: the app fetches third-party files, and their GPL/CC BY-SA terms do not reach the
  installer. A list is refreshed after four days; a download that fails or does not start with
  `[Adblock Plus` keeps the previous copy. Lists and the serialized engine live in
  `app_local_data_dir/content-blocking` and are derived: deleting the folder only costs a download.
  Without any list nothing is blocked.
- **Network rules:** WebView2's `WebResourceRequested` on the page window only, for every request
  source; a blocked request gets an empty 403. The main-frame document the user opened is never
  blocked, only what it loads.
- **Cosmetic rules:** hiding only (`display: none !important`), through an adopted stylesheet created
  in Rust's isolated world, never through a channel from the page: by a script registered for each
  main-frame document with `Page.addScriptToEvaluateOnNewDocument`, which applies the host's rules
  at document start and generic class and id rules as elements appear, and by later passes
  (`browser::read`). Scriptlets, procedural rules (`:has-text`, `:style`, `:remove`), `$redirect`,
  `$removeparam` and `$csp` stay off: they run code in the page or rewrite it.

**Consequences:**

- MPL-2.0 applies only to the crate's own files; their source is public on crates.io and the app's
  MIT license is unchanged. A change to those files must be published under MPL-2.0.
- The app contacts `easylist.to` and `secure.fanboy.co.nz` about every four days, without cookies,
  revealing only that some app downloaded the lists.
- A crate update may change the engine's binary format; the cache then fails to load and is rebuilt
  from the lists. A panic in the engine aborts the app (release builds use `panic = "abort"`), as any
  other dependency would.
- Hiding rules apply before a document's first paint, generic class and id rules as soon as an
  element carries them, for the document's lifetime. A navigation the page starts to another host
  of the same site may get its script after the document starts; its rules then follow a few
  hundred milliseconds after commit and generic ones within about 15 seconds. Each document pays
  about 10 ms to load the generic rules, and every frame compiles the 500 KB script. Each pass
  briefly enables `Runtime`, which a page can notice, as with page selection reads; the enabled
  `Page` domain is not visible to it.
- Requests from every frame are checked against the main frame's URL for third-party rules.

- The shield beside the address toggles an exception for the registrable domain (eTLD+1), using
  the engine's embedded public suffix list, including private suffixes. IP addresses and local names
  use the exact host; ports and HTTP/HTTPS share the exception. Changes reload the current site.
- Exceptions disable network and cosmetic rules for the allowed top-level document, including its
  third-party requests. They live in app preferences, outside the notes folder. WebView2's strict
  tracking prevention is profile-wide and stays enabled even on allowed sites, where it still
  blocks the large ad networks (Google Ad Manager included), so allowing a site mostly restores
  first-party content and smaller ad sources, not those ads.
