# 0014 — Secondary app windows

**Context:** M16 shows note images in a window beside the panel (`image-preview`), and M16's
floating images and M17's post-its add more (`float-*`). They load the same UI page as `main` and
show note content, so they need its protections ([0004](0004-untrusted-content.md),
[0009](0009-note-privacy.md)), but none needs `main`'s commands: a script that reached one of them
must not reach the notes folder, the panel or the page window. Tauri's capabilities do not cover app
commands from a local origin, and `note-asset` answers every WebView unless it checks the label. In
Tauri 2.12.5, `core:event:allow-listen` lets a window subscribe to events sent to any window, such
as `main`'s `notes-changed` with note paths, and `allow-unlisten` removes other windows' listeners.

**Decision:**

- **One hardened builder** (`main_window::app_window`) for `main` and every secondary app window:
  InPrivate (`incognito`), `Object.freeze(Object.prototype)`, navigation only within the UI origin
  (`on_navigation(stays_in_ui)`), no new windows (`on_new_window(Deny)`), no Tauri file-drop
  handler, excluded from capture while still hidden, and Edge's keys and menu items filtered
  (`edge::filter`, `Owner::Ui`). Secondary windows (`main_window::secondary_window`) also refuse
  every external drop through WebView2 (`SetAllowExternalDrop(false)`); `main` keeps WebView2's
  drops for images dropped into the editor. A window whose setup fails after it was built is
  destroyed, never reused half-hardened. The theme comes from settings; Rust's theme changes reach
  every window.
- **Command barrier by label** (`by_window` in `lib.rs`): `main` reaches `main`'s command list. A
  secondary window reaches only a disjoint secondary list, and each of those commands also checks its
  caller's label. Only `image-preview` and, from M16's floating windows on, the live `float-*`
  labels Rust registered count as secondary (`is_secondary_window`); any other window, `web` and
  `tweak` included, reaches no app command.
- **Images:** `note-asset` answers only `main`, `image-preview` and registered live `float-*`
  windows (`main_window::reads_images`); any other label, `web` and `tweak` included, gets 403.
- **No capability:** secondary windows get none, so the ACL refuses them every core command: no
  event listening or sending, no `core:window:*`. Rust decides their geometry, visibility and focus.
- **Updates over an IPC channel:** a secondary window hands Rust a `tauri::ipc::Channel` through its
  secondary command when it loads, and Rust sends it everything it must show through that channel,
  replacing an older one after a reload. Messages carry a generation, so the window keeps the latest
  whether it arrives as the command's answer or on the channel. `main` still uses events; Rust never
  broadcasts with `app.emit` and always names the window with `emit_to`.

**Consequences:**

- A new secondary window means a label rule in `main_window::is_secondary`, commands in the
  secondary list that check their label, and its own channel; `page_ipc.rs` tests the barrier and
  the ACL with the production rule.
- A secondary window cannot move, show or close itself, nor talk to `main` directly: anything it
  needs goes through a secondary command, and anything `main` needs goes through Rust.
- Channel messages up to 8 KB run directly in the window; larger ones would need Tauri's channel
  fetch command. A path is far below that.
- The image preview never takes focus or clicks: Rust shows it without activating and it stays
  disabled and click-through, so the panel's hover and focus rules ignore it.
