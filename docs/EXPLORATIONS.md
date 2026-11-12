# Explorations

Directions studied but not adopted, so they are not researched again from zero. Each entry says what
would make it worth reopening.

## The app's own encryption

Open formats are not the same as plaintext: a note encrypted with a documented standard keeps the
freedom to leave the app. The best fit found is [age](https://age-encryption.org): armored `age` code
blocks inside a note, which other Markdown apps show as code (as Emacs org-crypt does with GPG), or whole
notes as `.md.age`. Keys follow envelope encryption: a data key wrapped by the password (the root), by
Windows Hello through the TPM (convenience only) and by a printable recovery key. Biometrics alone would
lose notes with the device. Plugin-specific formats are fragile: Obsidian's Meld Encrypt dropped its
inline format in 2.0.

Not adopted because a mounted vault (Cryptomator) covers the threats without crypto code in the app.
Reopen if users need encryption of single passages or notes without a vault.

## Mobile app

Tauri 2 runs on Android and iOS, and about 40% of the Rust (notes and pinned logic) could be shared; the
panel, the edge, the tray, the shortcut and the web tab are desktop-only, and the edge panel, the app's
reason to exist, has no mobile counterpart. New work, roughly from largest:

- Sync: without it the app is useless. External tools (Syncthing on Android, few options on iOS) or
  cloud APIs with conflict handling.
- Touch UI: a different product from a keyboard sidebar.
- File access: Android reaches user folders through the Storage Access Framework, not paths, so
  `notes.rs` does not work there as is; iOS has similar limits. Needs Kotlin and Swift code.
- Cryptomator vaults: the format is documented and reading is feasible; writing risks corrupting a
  vault. The official library is AGPL, so an implementation would follow the specification. Cryptomator
  for Android does not let other apps open vault files, so reading vaults directly would be a real gain.
- Biometrics through the official Tauri plugin; store accounts and reviews.

Estimate against the app as of M5 (about 8,300 lines of Rust and 8,500 of UI): a read-only Android
viewer of a vault in a synced folder, about half; a full app with editing and sync on both platforms,
1.5 to 2 times, with maintenance growing faster than code. Reopen if Cryptomator's mobile app plus another
notes app proves insufficient; start with the read-only Android viewer.

## GPUI instead of a web view

GPUI, the Rust UI framework from the Zed editor, draws on the GPU (DirectX on Windows) with no browser:
lower memory and startup cost, and one language. It is pre-1.0 with frequent breaking changes, has few
ready-made widgets (Longbridge's gpui-component is the main library) and no HTML engine. DocDock depends
on HTML: the web tab needs WebView2 anyway, rendered Markdown and the CodeMirror editor would be
rewritten, and Zed's own editor crates are GPL, unusable in an MIT app. A rewrite of the whole UI for a
partial memory gain. Reopen only if WebView2 memory becomes a measured problem that its own measures
(see `docs/decisions/0001-tauri-svelte.md`) cannot fix.
