# 0009 — Note privacy through a user-mounted vault

**Context:** the threats that matter are leaks through cloud sync, a stolen device with synced notes
and screen sharing of a panel that appears over other apps. Notes are ordinary `.md` files
([0002](0002-content-in-files.md)), so encrypting them in the app would either lock them into the app
or require its own key management. Tools such as Cryptomator already encrypt a folder for cloud sync
and mount it as a drive while unlocked.

**Decision:** notes stay plain `.md`. Encryption is delegated to a vault the user mounts, which stays
optional; DocDock only behaves safely around it. Nothing derived from note content (text, names,
titles, tags, images, search indexes or caches) is stored outside the notes folder, so a vault
protects everything the app knows about the notes. When the folder is unavailable, such as a locked
vault, the app never creates it or its parents and keeps derived content out of memory. The panel is
excluded from screen capture, and copies from notes stay out of clipboard history and cloud
clipboard. The app's own encryption is in `docs/EXPLORATIONS.md`.

**Consequences:**

- Protection depends on the user's vault; the README explains how to set one up and that moving notes
  into a vault does not remove earlier plaintext from cloud history.
- App settings outside the folder hold only preferences and the folder path, never note content or
  names; derived state that needs persistence lives inside the notes folder or is rebuilt from it.
- Every write path must refuse to create a missing notes folder, and a missing folder must be told
  apart from a deleted one.
- Deleting must not move notes out of the vault, including on volumes without a Recycle Bin: there,
  deleted notes and folders go to a trash inside the notes folder (`.docdock/trash`), stay 30 days
  (encrypted in a vault and synced with it meanwhile), then are purged; Preferences restores them or
  empties the trash at once.
- The user's own screenshots of the panel are blank, and copies from notes do not appear in Win+V.
