# Explorations

Directions studied but not adopted, so they are not researched again from zero. Each entry says what
would make it worth reopening.

## The app's own encryption

Open formats are not the same as plaintext: a note encrypted with a documented standard keeps the
freedom to leave the app. The best fit found is [age](https://age-encryption.org): armored `age` code
blocks inside a note, which other Markdown apps show as code (as Emacs org-crypt does with GPG), or whole
notes as `.md.age`. Keys follow envelope encryption: a data key wrapped by the password (the root), by
Windows Hello through the TPM (convenience only) and by a printable recovery key; biometrics alone would
lose notes with the device. Plugin-specific formats are fragile: Obsidian's Meld Encrypt dropped its
inline format in 2.0.

Not adopted: a mounted vault (Cryptomator) covers the threats without crypto code in the app. Reopen if
users need encryption of single passages or notes without a vault.

## Mobile app

Tauri 2 runs on Android and iOS, but only the notes and pinned logic (under half the Rust) could be
shared; the edge panel, the app's reason to exist, has no mobile counterpart. New work, largest first:

- Sync: external tools (Syncthing on Android, few options on iOS) or cloud APIs with conflict handling.
- Touch UI: a different product from a keyboard sidebar.
- File access: Android reaches user folders through the Storage Access Framework, not paths, so
  `notes.rs` does not work as is; iOS has similar limits. Needs Kotlin and Swift code.
- Cryptomator vaults: reading the documented format is feasible; writing risks corrupting a vault. The
  official library is AGPL, so an implementation would follow the specification. Cryptomator for
  Android does not let other apps open vault files, so reading vaults directly would be a real gain.
- Biometrics through the official Tauri plugin; store accounts and reviews.

A read-only Android viewer of a vault in a synced folder costs about half the desktop app's code; a full
app with editing and sync on both platforms, 1.5 to 2 times, with maintenance growing faster. Reopen if
Cryptomator's mobile app plus another notes app proves insufficient; start with the read-only viewer.

## GPUI instead of a web view

GPUI, the Rust UI framework from the Zed editor, draws on the GPU with no browser: lower memory and
startup cost, and one language. It is pre-1.0 with frequent breaking changes, has few ready-made widgets
and no HTML engine. DocDock depends on HTML: the web tab needs WebView2 anyway, rendered Markdown and the
CodeMirror editor would be rewritten, and Zed's editor crates are GPL, unusable in an MIT app. Reopen
only if WebView2 memory becomes a measured problem that its own measures (see "Memory while parked" in
`docs/ARCHITECTURE.md`) cannot fix.

## Animated editor layout changes

Revealing Markdown marks under the pointer changes line heights. A spike tried animating it: CSS height
transitions left CodeMirror's height map stale by up to 108 px, breaking drags; FLIP with transforms
landed clicks one or two lines off for about 60 ms; FLIP with input remapping worked but made repeated
arrow keys heavier on slow CPUs, drew the caret ahead of the text and overlapped lines. Not worth the
complexity, so reveal changes stay instant after the pointer gesture. Reopen only if instant changes are
reported as disorienting.

## Region capture

Capturing any screen region is already covered by Windows' Win+Shift+S followed by Ctrl+V in a note, so
only the web tab is captured. Reopen if users need captures inserted without the clipboard.
