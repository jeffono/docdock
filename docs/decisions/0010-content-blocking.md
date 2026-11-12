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
  in Rust's isolated world (`browser::read`), never through a channel from the page. Scriptlets,
  procedural rules (`:has-text`, `:style`, `:remove`), `$redirect`, `$removeparam` and `$csp` stay off:
  they run code in the page or rewrite it.

**Consequences:**

- MPL-2.0 applies only to the crate's own files; their source is public on crates.io and the app's
  MIT license is unchanged. A change to those files must be published under MPL-2.0.
- The app contacts `easylist.to` and `secure.fanboy.co.nz` about every four days, without cookies,
  revealing only that some app downloaded the lists.
- A crate update may change the engine's binary format; the cache then fails to load and is rebuilt
  from the lists. A panic in the engine aborts the app (release builds use `panic = "abort"`), as any
  other dependency would.
- Hiding rules apply a few hundred milliseconds after a document commits and generic class and id
  rules follow as elements appear, for about 15 seconds; content added later keeps only the rules
  already applied. Each pass briefly enables `Runtime`, which a page can notice, as with page
  selection reads.
- Requests from every frame are checked against the main frame's URL for third-party rules.
