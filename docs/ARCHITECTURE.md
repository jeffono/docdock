# Architecture

How the system works: modules, how they connect and the invariants that span them. Details of each
module are in its code comments, visible behavior in `docs/DESIGN.md` and why costly choices were made
in `decisions/`.

## Structure

```text
src/                     UI (Svelte 5 + Vite)
  App.svelte             Screen composition, no business rules
  lib/ipc.ts             Only entry point that calls Rust
  lib/i18n.ts            Interface text in English and pt-BR, and error codes to text
  lib/clipboard.ts       Copy and cut kept out of clipboard history and cloud clipboard
  lib/panel-tabs.ts      Pure tab cycle and top-bar mouse-wheel switching rules
  lib/theme.svelte.ts    Theme setting applied as the root color scheme
  lib/features/<name>/   Components, state and tests: notes, scratchpad, web, tab (edge tab),
                         preferences, image-preview, floating, tweak-panel (development builds only)
  lib/ui/                Generic visual components
  lib/styles/tokens.css  Colors, spacing and typography
src-tauri/src/           Backend (Rust)
  lib.rs                 Only assembles the app and registers modules
  main_window.rs         Creates app windows (`main`, the image preview, floating windows) hardened alike
  preview.rs             Side image preview: pure placement beside the panel and its commands
  floats.rs              Pure floating-window rules: `floating.json`, fitting to monitors, Alt reveal
  floating.rs            Floating windows: registry, worker, commands and lifecycle
  panel.rs               Pure panel state machine
  handle.rs              Pure edge handle rules: where it sits, when it shows, which stretch opens
  motion.rs              Slide duration and easing; tunable live in development builds
  dock.rs                Thread connecting cursor, state machine and windows
  tray.rs                Tray: icon, screen side, exit and Windows startup
  shortcut.rs            Global panel shortcut
  web.rs                 Web page window, with untrusted content
  blocking.rs            Content blocking in the page window: filter lists, engine and hiding scripts
  permissions.rs         Site permission requests of the page window: rules, remembered choices, waiting
  reader.rs              Reader view: the page's article as Markdown
  screenshot.rs          Pure page capture rules: full-page clip, size limits, image validation
  paste_source.rs        Pure rule naming a note made from a paste after the window it came from
  notes.rs               Notes folder, settings, listing, reading, atomic saving, renaming, moving,
                         deleting with undo, leftovers, local images and `_pinned.md` reads and writes
  pinned.rs              Pure line rules for `_pinned.md`
  attachments.rs         Pure reference matching for attachment images
  trash.rs               The notes folder's own 30-day trash for deletes without a Recycle Bin
  i18n.rs                Interface language, from the Windows display language
  error.rs               Errors sent to the UI as codes
  updates.rs             Update check, download and the exit gate shared with Quit
  watch.rs               Tells the UI when notes or `.docdock/scratchpad.md` change, or the folder leaves/returns
  spelling.rs            Editor spell checking on its own thread, across the user's languages
  dev.rs                 Tweak window backend, development builds only
  commands/              Tauri commands: thin delegating layer
  platform/              Operating-system-specific code
```

```mermaid
flowchart LR
  F["features/*"] --> IPC["lib/ipc.ts"]
  IPC -- "invoke / events" --> C["commands/"]
  C --> D["pure rules"]
  C --> P["platform/ (Win32)"]
  D --> V[(".md notes folder")]
```

## Shared rules

- **Commands** are accepted from the `main` window and, each from its own disjoint list, from
  secondary app windows such as the image preview (`by_window` in `lib.rs`,
  [0014](decisions/0014-app-windows.md)). Events always name their window with `emit_to`;
  secondary windows have no capability and hear Rust only through an IPC channel they hand over.
  Errors are `{ code, ...fields }` (`error.rs`) and `i18n.ts` turns each code into text; `detail` carries the operating system's message. Commands that touch the notes folder run off
  the main thread, since an offline share can take seconds to answer.
- **Settings** live in `settings.json` in the app configuration directory and hold only preferences
  and the folder path, never note content or names ([0009](decisions/0009-note-privacy.md)). One
  locked writer loads, changes and saves them atomically; a missing or invalid field falls back to
  its default without invalidating the others. State that changes at runtime (site exceptions and
  permissions) is published to memory only after a successful save, and handlers read only memory.
- **Language:** Rust reads the Windows display language once at startup (any Portuguese gives pt-BR,
  everything else English); the tray uses it and the UI receives it before mounting.
- **Theme:** `system`, `light` or `dark`, read before mounting and applied as the root `color-scheme`,
  which tokens and the editor's shadow root inherit. Native windows receive it too, and Tauri forwards
  Windows theme changes to WebView2, including pages' `prefers-color-scheme`.

## Panel

`panel.rs` is a pure state machine; `dock.rs` drives it on one thread that polls the cursor every
50 ms, receives UI requests and animates, so opening, closing and animating never compete.
Coordinates are physical pixels with per-monitor DPI awareness (PMv2).

```text
Hidden → Arming (cursor at edge, waiting) → Open → Pinned
Open → Summoned (a click gives the panel or its page focus, or the shortcut)
Hidden → Summoned (by shortcut) → Dismissed (cursor still at edge) → Hidden
```

- **Windows:** `main` holds the UI; the web page is its own window (`web.rs`), owned by the panel
  ([0003](decisions/0003-panel-and-web-pages.md)). Both exist at their final size, parked beyond the
  screen edge, and slide together in one `DeferWindowPos` transaction, frame by frame after
  `DwmFlush`, along the curves in `motion.rs`; the WebView is never resized. The parked position lies
  outside every screen, and the window region is clipped to the monitor's columns each frame (empty
  while parked). Layout, resolution, DPI or taskbar changes, read at each poll, move the windows
  without animation.
- **Trigger and edge handle:** pure rules in `handle.rs` decide the outermost monitor on the chosen
  side, the activation stretch, the reveal area, the rush and where the cursor counts as inside the
  panel (including taskbars and app bars in its columns, until they are used). The handle is the
  panel's tab, drawn by the panel's own WebView (`features/tab/EdgeTab.svelte`): `main` is
  transparent and wider than the panel by the tab's depth, and `SetWindowRgn` shapes it to the panel
  plus the tab's outline so clicks beside the tab reach the windows below. Rust gives the UI the tab's
  size (`panel_tab_shape` and the `panel-tab-shape` event), so the geometry has one source. Because
  the viewport reaches beyond the panel, the UI places dialogs, menus and tooltips within the
  panel's columns (`ui/place.ts`). While parked the window is click-through
  (`WS_EX_LAYERED | WS_EX_TRANSPARENT`); only the open, resting tab takes clicks, sent as `panel_tab`.
- **Focus:** the window has `WS_EX_NOACTIVATE` and `WS_EX_TOOLWINDOW` and opens by cursor without
  activation. A UI `pointerdown` requests activation, so the panel becomes summoned at the next poll;
  then focus governs, not the cursor, and the panel closes when focus moves to another app or the UI
  asks (`panel_close`). A focused page counts as panel focus; null focus, common during system
  transitions, is ignored. Pinned, neither cursor nor focus closes it. Each poll remembers the last
  foreground app outside the panel, which gets focus back on close.
- **Hidden panel:** the window is disabled, because when the foreground app closed, Windows activated
  the off-screen panel despite `WS_EX_NOACTIVATE`. After the slide out, `dock.rs` ends the session
  (Undo copies, shortcut recording, trash purge) and emits `panel-hidden`.
- **Esc:** in `main` the UI owns it, one layer at a time, and only the last layer asks Rust to close.
  Rust reads the key only for a focused page, which has no channel to Rust.
- **System menu:** `platform::suppress_key_menu` swallows `WM_SYSCOMMAND(SC_KEYMENU)` on `main`, and
  `platform/edge.rs` disables WebView2's `Alt`-release accelerator in `Owner::Ui`, so `Alt` and
  `Alt+Arrow` preview navigation never trigger the Windows window menu while keeping `Alt+F4`.
- **Focus band:** Rust emits `panel-focus` when the panel window or its page gains or loses the
  foreground; DOM focus events cannot see the page window.
- **Shortcut** (`shortcut.rs`): registered by Rust as virtual keys, so the UI's physical key is
  mapped to the layout's key first (`platform/keyboard.rs`). Validation is a pure function. Recording
  unregisters the current combination and always ends: on losing the foreground or hiding, after 30
  seconds, or when a reloaded UI reads the state. A saved combination that fails at startup falls
  back to the default for the session. The press never closes the panel by itself: Rust emits
  `panel-search` with whether the panel was already foreground, and the UI decides between focusing
  the search bar and asking to close. The key also grants `SetForegroundWindow`; if activation still
  fails, the cursor rule applies. With the native folder picker open, the shortcut and the tab do
  nothing: the picker is modal and would re-enable the hidden window.
- **Capture exclusion** ([0009](decisions/0009-note-privacy.md)): `platform/capture.rs` sets
  `WDA_EXCLUDEFROMCAPTURE` (or `WDA_MONITOR` before Windows 10 2004) on each top-level window while it
  is still hidden, including every recreation of the page window. Best effort: failures are logged.
  Chromium-native popups (the default context menu, `<select>` lists) are separate windows it does not
  reach.
- **Memory while parked:** `platform/memory.rs` sets WebView2's memory target to low on both webviews
  while parked and back to normal when a slide starts; `dock.rs` derives the level from that state.
  Scripts keep running, so autosave and file events stay live; `TrySuspend` would pause them.
- **Side preview** ([0014](decisions/0014-app-windows.md)): one reusable `image-preview` window,
  created hidden on first use by an asynchronous command and handed to `dock.rs`, which alone shows,
  places and hides it, so it never races a slide. It serves grid images (`Kind::Image`), closed
  markdown notes (`Kind::Note`), and the opaque post-it drag ghost (`Kind::PostIt`). It is a tool window that never activates, stays
  disabled and is click-through for good, so it is not one of the panel's windows for focus, hover,
  slides or memory. The UI (`features/image-preview/trigger.ts`, pure) sends the element's CSS box
  (plus natural size for images via `image_preview_show`, or note path via `image_preview_show_note`);
  `preview.rs` turns them into physical pixels from the panel window's origin and DPI and places the
  callout beside the panel, past the edge tab (`preview::place` or `preview::place_note` up to
  `NOTE_SIZE` 360 × 400); the dock publishes where the frame's arrow points to the preview window and
  shows the window in one step under the preview's lock (`apply_placement`). For notes, `ImagePreview`
  fetches the text via `image_preview_note` (allowed only from `preview::LABEL`), clips it via pure
  `note-clip.ts` (100 lines / 12,000 chars), renders through `renderNote`, and shrinks the bubble height
  via `ResizeObserver` around `arrowY` (except `Kind::PostIt`, which renders full post-it sheet framing). When `note_preview` is `Alt`, `dock.rs` polls
  `platform::alt_held()` every 50 ms while the panel is open in Notes and emits `panel-alt` to `main`
  so `Alt+hover` (including unfocused) and `Alt+focus` work immediately. It shows only over the resting open panel in Notes without a
  dialog; hiding the panel, a placement change (monitor, side, tab, width), showing the page, a
  dialog, the shortcut, dragging, the folder going away or another folder chosen hide it, and Rust
  then tells both windows (no path on the preview's channel, `image-preview-closed` to `main`, also
  for a show still on its way, so `main` drops the shows it queued after it). Every show, hide and
  close starts a new generation: a show publishes its target only while its generation is current and
  no other folder was chosen since it validated the path, and the dock thread checks the generation
  and shows the window under the same lock, so a slow show never outlives a hide or a locked vault.
  That lock is never taken on the main thread, which showing the window waits for. What the preview
  window hears carries a separate version, which changes only with the target shown. The window
  decodes the next image before swapping and drops any image or rendered note DOM when it hides.
- **Floating windows** ([0014](decisions/0014-app-windows.md)): up to 8 `float-<n>` windows, of
  two kinds: images and post-its (notes shown read-only). `floating_open` validates the image, reserves a label in the registry,
  asks the dock thread where the preview would show the image (`Request::PlaceFloat`, through
  `preview::place` and `preview::image_rect`) and builds a hidden secondary window, a tool window
  that never activates but stays enabled (`platform::prepare_floating`). Under `SAVING` it checks
  that the folder, the image, the epoch and the window are all still there and saves the entry;
  only then does it show the window topmost with `SetWindowPos` (never Tauri's `show` or
  `set_focus`), and only if no close or folder change came meanwhile (`floating::save_and_show`).
  A close after the save removes the entry itself, and a folder change after it destroys the window
  but keeps the entry, rightly saved in the folder it opened on, so nothing is rolled back across
  folders. Unlike the
  preview, it is not the dock's: it never slides, and clicking or dragging it changes no
  foreground, so the panel's hover and focus rules are unaffected. Dragging and resizing go
  through Tauri's `start_dragging` and `start_resize_dragging`, from the window's own commands.
  The window is the image plus a transparent `floats::SHADOW_PAD` gutter on every side, and keeps
  the image's aspect ratio: from the natural size on opening, from the saved size on restoring,
  and from the image itself once it loads (`float_natural`), which shrinks a window saved with
  other proportions to fit the image without moving its corner.
  A `WM_SIZING` subclass, installed on the window's thread (`platform::constrain_sizing`), lets
  `floats::constrain_sizing` decide every rectangle of a resize, the work area winning over the
  minimum size; `float_zoom` grows or shrinks a window already shown around its center within
  `floats::zoom_rect`'s bounds, never showing, raising or activating it, and with no Win32 call
  under the registry's lock; placing and restoring reshape it with
  `floats::contain`. Later windows cascade down and away from the panel, within the room beside it
  (`floats::cascade`). Zooming inside the window and panning are the page's own
  (`features/floating/zoom.ts`). `main` hears which images open windows show (`floating_list`,
  then `floating-paths`, emitted whenever a window opens, closes or goes) and marks them with a
  badge laid in each rendered image's corner (`features/floating/detach.ts`), which closes their
  windows (`floating_close_path`); paths compare as `attachments::normalize` does. Closing by
  path, from the tray ("Close all floating windows") or one window at a time all go through the
  same user close.
  A post-it (`floating_open_note`, `floating_drag_start`/`floating_drag_end`, `PostIt.svelte`) opens the same way beside
  the note's card, or via pointer-capture drag (`floating_drag_start` preparing either
  `Kind::PostIt` via `preview::begin_post_it` or grabbing an open `Existing` post-it window, tracked
  on `wait_frame()` by `dock.rs::step_note_drag`, keeping the panel held open outside the edge and
  cancelling on Esc / secondary button via `panel-drag-cancel`, then spawning at `rect` and hiding
  the ghost seamlessly once `PostIt` calls `float_ready`) at `floats::NOTE_SIZE`, resizes
  freely (`floats::constrain_free`) and never zooms; Ctrl+wheel sizes
  its text, saved as the entry's `textScale`. It reads its note through `float_note`, only from the
  folder it opened on (`notes::floating_note`), and again whenever Rust raises its `revision`:
  `watch.rs` hands every change it reports to the worker (`floating::notes_changed_requested`),
  which first moves the post-its of the folder it watched, live and saved by id, along the renames
  it saw there, in their order; a read that a rename or folder change overtook is dropped. `float_edit` asks the dock
  thread for `Request::EditNote`, which summons the panel as the shortcut does and emits
  `panel-edit-note` to `main` with the post-it's epoch; the panel opens the note only while
  `floating_edit_current` finds that epoch current and no folder was chosen in the panel since it
  asked, so a request overtaken by a folder change is dropped. Commands of one kind refuse windows of the other.
  Click-through is `WS_EX_LAYERED | WS_EX_TRANSPARENT`; while any window has it, one worker thread
  polls every 50 ms and, with Alt (not AltGr) over a window, clears it until Alt and any drag it
  started are released (`floats::reveal_step`). The same worker saves moves, resizes and fades
  500 ms after they pause, and once more in `exit_ready`, off the main thread, before the exit gate
  lets the app go; exit waits for that save. The registry is never locked on the main thread, since its holders change window
  styles, which waits for it: the main thread's window events and the tray's unlock only message
  the worker, and the command barrier and `note-asset` read a separate set of live labels held
  only for the lookup.
- **Floating windows and the notes folder:** each window belongs to the folder it opened on, by
  path and marker id (`notes::FolderIdentity`), and to the registry's epoch. A folder leaving
  (`watch.rs`) starts a new epoch: every label leaves the live set first, so its commands and
  images stop at once, then every window is destroyed without touching the file
  ([0009](decisions/0009-note-privacy.md)). Choosing or trusting a folder does the same before the
  settings change (`floating::begin_switch`), holds every opening and restore off until it ends,
  then starts another epoch and reopens the configured folder's windows. Work started on an
  earlier epoch never registers or shows a window, nor saves a move or a pruned entry. A close
  marks its window first and forgets its entry before destroying it; a revocation that overtakes
  it forgets the entry itself, while the folder is still configured, so a closed window never
  returns with its folder.
  `note-asset` answers a floating window only while the configured folder is still the one it
  opened on. Every read and write of `.docdock/floating.json` checks, under `SAVING`, as the
  attachment cleanup holds it, that the folder is still the identity it started on, and fails with
  `FolderChanged` against any other, including one chosen at the same path. Lock order: `SAVING`,
  then the floating registry. The file (`floats.rs`, version 1) keeps each window's id, kind
  (`image` or `note`), path, physical rectangle, opacity and a post-it's text size, and entries of
  unknown kinds as they are; a file of another version is
  neither opened nor overwritten, and a corrupt one reads as empty. Startup and the folder's return
  reopen windows unlocked, sized to and then kept on the monitors there are now
  (`floats::fit_to_monitors`), dropping entries whose image or note is gone and keeping those that
  cannot be checked. Closing a window forgets its entry; a window destroyed otherwise keeps it.
- **Tweak window** (development builds only, `dev.rs`): a `tweak` window on the same UI page with
  event permissions only; app commands still reject it. It sends its state to the panel as
  `tweak-state`, and `dock.rs` reads the live tuning, or the constants in release builds, once per
  poll. Release builds contain neither the module, the commands nor the frontend code. Its UI is in
  "Tweak window" in `docs/DESIGN.md`.
- **Limit:** the panel works over windowed and borderless full-screen apps, not exclusive-mode games.

## Tray, startup and updates

`tray.rs` and Preferences share the screen side and Windows startup operations, and each emits an event
so the other follows. Startup is the `DocDock` value in the current user's `Run` key
(`platform/autostart.rs`), quoted, never duplicated in `settings.json`; changing it clears Task
Manager's earlier choice, and at startup an entry naming a missing or unquoted executable is pointed at
the running one. The tray glyph follows the taskbar's theme and DPI (`platform/taskbar.rs`). A second
instance exits.

`updates.rs` checks at production startup, every six hours and on Windows resume; development builds
disable updates. Triggers share one check or download, and Rust verifies the download and keeps it in
memory. Quit and update share a native exit gate: an event asks the UI to freeze input and finish both
the note editor and the Scratchpad through their save guards, and only its acknowledgement allows exit;
busy operations, save errors and conflicts keep the process open ([0008](decisions/0008-distribution.md)).
Windows NSIS relaunches after installing.

To cut a release, align package, Cargo and Tauri versions and `Cargo.lock`, commit, push through normal
review, then manually run Release on main; the workflow rejects mismatched versions or a stale
`Cargo.lock` before signing. Inspect the draft's assets and publish it as a regular latest release;
never publish an older draft as latest after a newer one. Local release builds also need
`TAURI_SIGNING_PRIVATE_KEY` (file path or contents) and the matching password variable. `pnpm icons`
rebuilds the bundle icons and tray glyphs from `src-tauri/icons/source/`.

## Notes

### Folder and its availability

`notes.rs` lists `.md` files recursively, skipping hidden folders, symlinks and junctions. A subfolder
that cannot be read is skipped and reported (`skipped`); the UI keeps what it knew from it
(`features/notes/listing.ts`) instead of treating it as deleted. Rust sends raw text to the UI, which
has no filesystem permission.

A locked vault, an unplugged drive or an offline share takes the folder away, and creating it again
would write plaintext under a vault's mount point ([0009](decisions/0009-note-privacy.md)). So:

- `notes_folder` reports `available`, `unavailable` or `missing` (pure `availability`): available only
  if it lists, missing only when its parent lists without it.
- The folder carries an identity marker, `.docdock/folder-id`, and `settings.json` records the id
  expected. Choosing a folder is the trusted moment that adopts or creates it; a folder whose marker
  is absent or different is `unrecognized`, never written to or marked automatically, until the user
  confirms it (`notes_trust`). Settings from before markers adopt an existing one. Pure rule:
  `identity`.
- Every command starts from `root`, which opens the folder, checks the marker and holds the handle
  until the command ends, so the folder cannot be renamed or deleted meanwhile. No write path creates
  the folder or its parents.
- Accepted gaps, outside 0009's threats: writes still go by path, so a forced unmount between the
  check and a write, or a junction redirected after the check, can affect one write. Handle-relative
  writes would close them at a cost not justified today.
- Reading a note for the editor or a post-it, or an image for `note-asset`, resolves it below where
  the folder's handle really is (`Root::real`) and reads only if the file opened really is there
  (`read_resolved`), so a junction swapped in meanwhile, anywhere on the way, is refused. The search
  index's bulk reads go by path.
- `watch::monitor` checks the state every 2 s and emits `notes-changed` and `scratchpad-changed` on a
  change. On leaving, the UI (`folderAction` in `features/notes/folder-state.ts`) forgets every note's
  text, name and tag, and Scratchpad forgets clean text as well; Rust forgets what it keeps for Undo and
  closes the image preview and the floating windows, which reopen on return. The one exception is an
  editor with unsaved text, which stays in memory until saved on return or discarded.

### Writing without losing content

- **Versions:** `notes_open` returns the raw text and a version (a hash of the bytes, valid in one
  run). `notes_save` writes only if the file still has the base version and answers `conflict`
  otherwise; saves are serialized and the check is confirmed against the version actually replaced.
  An empty base expects absence. Text goes to disk exactly as received.
- **Atomic writes:** temporary and backup files sit beside the target with a non-`.md` extension, and a
  write never deletes a file it did not create. A note goes through `ReplaceFileW`; if the backup is
  not our base version, an external write slipped in and is put back. Settings and derived state use a
  synced temporary file and rename.
- **New files** go through `move_new`, which never replaces an existing file. Parent folders are held
  against rename and checked for links while writing.
- **Leftovers:** a save interrupted between its renames leaves a `.docdock-bak`. Opening a folder lists
  them and offers to restore each under a free name; nothing is overwritten or deleted.
- **Autosave** (`features/notes/autosave.ts`): one save in flight, the latest edit queued; a conflict
  stops saving and keeps the text. Every way out of editing goes through one guarded leave that
  flushes and refuses on conflict or error, including hiding the panel, choosing a folder, Quit and
  updating.
- **Scratchpad** (`features/scratchpad/Scratchpad.svelte`): backed by `.docdock/scratchpad.md`
  inside the notes folder (`scratchpad_open`, `scratchpad_save`), sharing `ReplaceFileW` conflict
  handling, vault protection (`0009`: missing files create nothing until non-empty content is
  written) and the exit gate. `watch.rs` emits `scratchpad-changed` without triggering a note
  relist, while external edits reload a clean Scratchpad as `Updated from disk`.
- **External changes:** `watch.rs` groups `notify` events into one `notes-changed` with the renames it
  could pair. The UI follows renames, then rereads the listing, `_pinned.md` and the notes it shows;
  only a clean editor takes new text. Reads started before a folder change or rename are dropped.

### Rename, move and delete

- **Rename and move** share one move (`relocate`) under the save lock with a version check; a taken
  name answers `renameConflict`, and pinned items and collapsed state follow. The editor holds saves
  while a move runs, so none reaches the old path. A move rewrites the note's relative links for its
  new folder (`movedLinks` in `features/notes/links.ts`), as an edit in the editor or a versioned save
  that never recreates the note. Links in other notes are not rewritten.
- **Delete** goes to the Recycle Bin (`platform/recycle.rs`), whose copy is read back and kept in
  memory for Undo until the panel hides; meanwhile the path is reserved, so a late save cannot
  recreate it. Where the bin would delete for good (a vault, a network share, a full bin), the item
  goes to the folder's own trash (`trash.rs`), inside the notes folder, kept 30 days, restorable from
  Preferences; the shell's decision for each item is final (bin missing, item over its limit or bin set
  to delete at once). Folders go the same way, with Undo only from the trash. Pinned items are removed
  and Undo restores them. Whether the shell can address the folder at all is asked once per folder
  (`remembered_reach`), warmed with the trash purge: a vault mounted by WinFsp takes about 2 s to
  refuse, and its items go straight to the trash. Only a refusal of the path itself is remembered,
  until another folder is chosen; any other failure tries the bin as before.
- **A note's images** follow it after the panel hides: those in `attachments/` folders that it
  references and no other note does, live or in the trash, by the reference rule of "Unused images".
  The delete moves only the note and records their paths as pending (`Pending`), with the folder's
  marker id and the panel session, never their bytes, so an Undo needs nothing for them; it drops the
  record. A hide ends the session (`forget_deleted`); the UI then leaves the editor the guarded way
  and, once that save succeeded, calls `notes_send_pending`, so a note that just gained a link to an
  image is on disk first. The request names the session its hide ended, so a late one never takes a
  later session's images. Only records of ended sessions go, and only if the folder is still the one
  recorded, by path and, under the save lock, by marker id and no folder chosen since. Each is checked again under that lock,
  ignoring only the deleted note's own trash entry, and the images still unused go: into that entry
  under `.images` ([0013](decisions/0013-trash-entries-with-images.md)), so restoring the note brings
  them back, or, where the note went to the bin or the entry is confirmed gone, as "Send to bin" sends
  them. Any doubt or error keeps an image in place, only logged, where "Unused images" lists it; so
  does a folder going away or chosen anew, a failed save, or the app closing first.
  Accepted gaps: the record holds paths, not file identities, so an image another app replaces under
  the same name before the panel hides goes in its place (to the bin or the entry, recoverable); and a
  sync client holding an entry's folders or adding a conflicted copy of its note can leave images in
  an entry that keeps its old date or does not restore from Preferences.
- **Restoring an entry with images** returns each to its own path, never under a free name its note's
  link would miss nor through a link (`place_image`), each on its own. One whose name is taken, that
  cannot move or that is no image, such as a file another app left there, stays in the entry and never
  holds back the note or the other images. The entry then holds only images, lists and restores as an
  item, and is renamed as made now (`trash::renew`), so its 30 days start again; the answer says so
  (`imagesLeftInTrash`, with their count) while the note is back.
- **The save lock** (`SAVING`) also covers emptying and purging the trash, so an entry never goes while
  a restore or a session's end moves something in or out of it. `DELETED`, the memory for Undo, is
  only ever locked after it.

### Unused images

The cleanup considers PNG, JPEG, GIF and WebP files under folders named `attachments` at any depth,
ignoring case and skipping hidden folders, symlinks and junctions. An image is in use if a Markdown
or HTML link resolves to it relative to the note, or the note contains its filename as a substring.
Matching ignores case and recognizes percent encoding. Notes in the folder's trash use their
original paths, including notes inside deleted folders. An image a path in `.docdock/floating.json`
names is in use too, whatever the file's version, so deleting a note never sends away the image a
floating window shows: both paths compare once resolved (either separator, `.` and `..`, empty
parts, case) and as the file the saved path reaches. The scan reads Markdown text in Rust and keeps
it in memory only. A scan that cannot read a note or folder stops. Before moving any selected image,
Rust scans again under the save lock and skips images now in use. Accepted gap: a reference through a
Windows 8.3 short name (`MAPA~1.PNG`) is not recognized; DocDock never writes one.

### Pinned items

Decision in [0005](decisions/0005-pinned-items.md). `pinned.rs` is pure text in, text out, and keeps
every non-item byte and line terminator. `notes.rs` reads and writes the file with the notes' atomic
replacement: pin and unpin retry on a concurrent change, while reordering requires the displayed
version and fails with `pinnedChanged` otherwise. Collapsed state and the restore copy live in
`.docdock/` inside the notes folder; older AppData copies migrate once the folder is available and
recognized.

### Search and list

`features/notes/search.ts` keeps an in-memory index in the UI, synced from `notes_list` modification
times and `notes_texts`; it tracks `hasGallery` using `markdown.ts`'s `hasImageGallery`, and nothing
about tags or text is persisted. `query.ts` parses free text, `#tag`, `date:`, `is:link` and
`has:gallery` tokens (rules in "Search filters" in `docs/DESIGN.md`). `features/notes/list.ts` is
the pure
model of cards, sections and keyboard actions; `Notes.svelte` runs the effects. The edited card keeps a
fixed key, so pinning, a `_pinned.md` change or a rename moves it instead of rebuilding the editor.
`tauri.conf.json` disables native file-drop interception so HTML drag and drop works.

### Rendering and editing

Both views use one parser ([0007](decisions/0007-one-markdown-parser.md)). Reading turns the syntax tree
into an allowlisted model of DOM nodes, never HTML. `MarkdownEditor.svelte` holds the decoupled
CodeMirror 6 editor core in an open shadow root (reused by `Editor.svelte` with frontmatter and local
image paths, and `Scratchpad.svelte` without them), because its styles must load as adopted or linked
sheets under the CSP. It tracks raw text apart from CodeMirror's offsets so mixed line endings survive
edits, reloads, undo and redo; no Markdown is serialized. Code fences load local Lezer grammars on
demand (`code-colors.ts`) for both views.

**Spelling** uses the Windows Spell Checking API (`spelling.rs`), not WebView2's checker, which drops
its marks whenever CodeMirror redraws a line and checks only WebView2's languages. The UI sends single
words, never note text, and a word is misspelled only when every language flags it.

**Pasted images** and image files dropped in the editor arrive as a raw IPC body. Rust checks the
signature and size. Paste and page capture share the attachment writer, which names the image after
the note's file stem as a short plain slug (lowercase, accents folded, words joined by hyphens, cut at
one within 30 characters) and takes the first free name for that extension (`my-note.png`,
`my-note-2.png`, …) in `attachments/` beside the note. `move_new` prevents overwrites, and the relative Markdown link percent-encodes the
file name. Zeros after a JPEG's or PNG's end, which Edge pads dragged images with, are dropped. After
one failed IPC request Tauri sends every later one as JSON until the page reloads, so the image
commands also take bytes as a JSON array of numbers. Drops from other apps (`features/notes/drop.ts`)
are read during the drop event; appending reads the note once, before attaching images, and saves
with that version: a note changed, moved or replaced meanwhile is left alone with `dropChanged`, and
images already attached are left to the unused-image cleanup. **Copy and cut** in `main` go through
`lib/clipboard.ts` and `platform/clipboard.rs`, which mark the text to stay out of clipboard history
and cloud clipboard.

### Note content on disk

`main` runs InPrivate (`incognito(true)`); the page window keeps a normal profile under
`%LOCALAPPDATA%/com.jeffono.docdock/EBWebView`. Note text reaches the UI through IPC and becomes DOM
nodes, `note-asset` answers with `no-store`, and release builds use no `localStorage`, IndexedDB,
service workers or Cache Storage (only the development tweak window keeps presets in `localStorage`).
These are source-level guarantees: GPU caches, crash dumps and paging are the runtime's, and the app
never deletes the shared profile, which would erase website sessions.

## Untrusted content

Decisions in [0004](decisions/0004-untrusted-content.md) (CSP, `main` navigation, `note-asset` and
capabilities) and [0003](decisions/0003-panel-and-web-pages.md) (the page window).

- **Page window IPC (as of Tauri 2.12.5):** Tauri injects its scripts into every WebView and frame, so
  pages see `window.ipc` and `__TAURI_INTERNALS__.invoke`, but every call fails: no capability names
  the `web` window, so the ACL rejects plugin and core commands and remote-origin app commands, and
  `by_window` rejects local-origin ones (the dev UI or `note-asset` loaded in the page). Never grant
  the `web` window a capability. Plugins must not add init scripts that change web APIs:
  `tauri-plugin-dialog` replaced the page's `alert` and `confirm`, so native dialogs use `rfd`, and
  opener's link script is disabled.
- **Navigation guard:** `on_navigation` sees only the main frame, so `platform/guard.rs` cancels iframe
  navigations to app origins and every external scheme launch. The page window is created blank and
  navigates only after the guard, observers, content blocking, permission handler and mobile identity
  are installed; one lock covers the open, and a window whose setup failed is never navigated.
- **Reading from the page:** Rust evaluates fixed scripts in an isolated world through WebView2's
  DevTools protocol (`platform/browser.rs`), never `ExecuteScript`, addressing the context by its
  `uniqueId` so a read never lands in another document. Reads take turns, run only between
  navigations, are dropped if the page's revision changed, and the response is bounded and checked as
  untrusted. The page selection, the reader view ([0011](decisions/0011-reader-view.md)) and cosmetic
  filtering use this path.
- **Edge features** (`platform/edge.rs`): accelerator keys and context menu items outside an allowlist
  are filtered natively in both windows, because Edge's dialogs either cannot close in the panel or
  take focus from it. Ctrl+Tab and Ctrl+Shift+Tab on the page emit `panel-toggle-mode` (`1` / `-1`) to `main`.

## Browser

- **Browser mode:** a shown page replaces the note list; hiding it disables its window but keeps its
  document and history. `platform/browser.rs` observes the page and emits `web-state` (URL, title,
  history) and `web-visible` to `main`; the page sends nothing.
- **Address bar:** `suggestions.ts` holds the pure intent and selection rules. Rust queries the
  provider's suggestions without cookies, referer or redirects, bounded and as text
  ([0006](decisions/0006-search-suggestions.md)), and builds result URLs from the chosen provider or a
  custom `%s` template. Visited addresses go to `address-history.json` in the app configuration
  directory. Suggestions and bars over the page cut a region out of the page window (`web_overlay`)
  instead of moving it.
- **Mobile identity:** before its first navigation the page gets an Android user agent and matching
  Client Hints through `Emulation.setUserAgentOverride`, keeping the installed engine versions; `main`
  keeps its own.
- **Content blocking** ([0010](decisions/0010-content-blocking.md)): a `blocking` thread loads the
  cached engine or rebuilds it from the cached lists, and refreshes lists in the background.
  `platform/blocker.rs` filters every request of the page window with `WebResourceRequested`, never
  the document the user opened. Cosmetic hiding registers a document-start script per main-frame
  navigation and follows up with isolated-world passes after commit. The page profile uses WebView2's
  balanced tracking prevention. Site exceptions (`blocking::site_key`, eTLD+1) live in settings.
- **Site permissions** ([0012](decisions/0012-site-permissions.md)): `platform/permission.rs` answers
  every request of the page window and resets what the profile saved before the first navigation;
  `permissions::decide` is the pure rule. Waiting requests are tied to their document and an on-screen
  epoch that changes on every show and hide; while one waits, the panel stays open as for a dialog.
- **Page captures:** native items in the page's context menu emit `web-capture` to `main` only, with
  the revision the menu opened on; the page cannot trigger it. The engine renders the image
  (`Page.captureScreenshot`), not the screen, one capture at a time, failing if the page navigates;
  `screenshot.rs` bounds the size. `notes_create_with_image` refuses if the folder changed since the
  capture started, writes the image then the note, never overwrites and never deletes.
