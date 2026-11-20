# 0008 — Distribution and updates

**Context:** the first public version was `0.1.0-beta.1`, a preliminary Windows beta. Distribution must
support in-place updates without losing pending edits. WiX/MSI rejects prerelease version strings.
Windows code signing and the updater's artifact signatures solve different problems.

**Decision:**

- Version with Semantic Versioning as `0.MINOR.PATCH` until 1.0: a 0.x version already says the app
  is not stable, so releases carry no `-beta` suffix (`0.1.0-beta.3` was the last). A milestone with
  new features raises MINOR; a release with only fixes raises PATCH. 1.0.0 comes when the app is
  considered stable. SemVer orders `0.2.0` after `0.1.0-beta.3`, so installed betas update normally.
- Ship only NSIS, installed for the current user, avoiding elevation for normal updates. Download the
  WebView2 bootstrapper when needed; installation needs internet if WebView2 is absent. Offer English
  and Brazilian Portuguese installer languages.
- Distribute unsigned installers through GitHub Releases and accept SmartScreen's unknown publisher
  warning before 1.0. Code signing options are in "Later" in `docs/PLAN.md`.
- Use `tauri-plugin-updater` from Rust only, with signed artifacts and a committed public key. Keep
  the private key outside the repository and in GitHub Actions secrets; back it up securely. No
  updater permissions or bindings are granted to either webview. No process plugin is necessary:
  Windows NSIS performs the restart.
- Check, download and verify in the background. Offer installation only after download and require
  an explicit restart action. Ordinary Quit does not install. Offline and invalid updates do not
  interrupt note use.
- Both Quit and updating require the trusted main UI to finish the editor, including image paste,
  pending rename and autosave. Freeze input during that handshake. Conflicts and save failures abort
  exit; active content operations refuse exit until retried. No timeout authorizes an exit.
- Use `https://github.com/jeffono/docdock/releases/latest/download/latest.json`. GitHub excludes
  prereleases from `latest`, so publish this single channel as a **regular GitHub release**, with
  the release notes saying that 0.x versions are not yet stable. Drafts never reach the channel.
  Do not mark published channel releases as prereleases. Keep this channel through 1.0; there is no
  separate stable audience yet.
- The release workflow builds and verifies on Windows and creates a draft containing the NSIS
  installer, signature and `latest.json`. Maintainers inspect the draft and publish it as the latest
  release. It runs only by manual dispatch from main: publishing the draft creates the `v<version>`
  tag, so a tag trigger would build a second, duplicate draft.

**Consequences:**

- Users may have to choose SmartScreen's "More info" then "Run anyway"; the installer has no verified
  Windows publisher. Updater signatures still authenticate downloaded updates and are mandatory.
- Losing the signing key prevents updates for installed clients; never rotate it without planning a
  transition. The generated key has no password; keep its file and backup private. GitHub secrets:
  `TAURI_SIGNING_PRIVATE_KEY` (the file contents) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (empty or
  absent for this key). GitHub's supplied `GITHUB_TOKEN` creates the draft.
- The tray item is disabled until a verified download exists. An unresponsive frontend blocks exit
  rather than risking content; force termination by the OS remains outside this guarantee.
- Schedule, controls and release steps are in "Tray, startup and updates" in `docs/ARCHITECTURE.md`.
