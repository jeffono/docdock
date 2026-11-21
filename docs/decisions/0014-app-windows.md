# 0014 — Secondary app windows

**Context:** M16 shows note images in a window beside the panel (`image-preview`), and M16's
floating images and M17's post-its add more (`float-*`). They load the same UI page as `main` and
show note content, so they need its protections ([0004](0004-untrusted-content.md),
[0009](0009-note-privacy.md)), but none needs `main`'s commands: a script that reached one of them
must not reach the notes folder, the panel or the page window. Tauri's capabilities do not cover app
commands from a local origin, and `note-asset` answers every WebView unless it checks the label.

**Decision:**

- **One hardened builder** (`main_window::app_window`) for `main` and every secondary app window:
  InPrivate (`incognito`), `Object.freeze(Object.prototype)`, navigation only within the UI origin
  (`on_navigation(stays_in_ui)`), no new windows (`on_new_window(Deny)`), no file drops, excluded
  from capture while still hidden, and Edge's keys and menu items filtered (`edge::filter`,
  `Owner::Ui`). The theme comes from settings; Rust's theme changes reach every window.
- **Command barrier by label** (`by_window` in `lib.rs`): `main` reaches `main`'s command list. A
  secondary window reaches only a disjoint secondary list, and each of those commands also checks its
  caller's label. Only `image-preview` and, from M16's floating windows on, the live `float-*`
  labels Rust registered count as secondary; any other window, `web` and `tweak` included, reaches
  no app command.
- **Images:** `note-asset` answers only `main`, `image-preview` and registered live `float-*`
  windows (`main_window::reads_images`); any other label, `web` and `tweak` included, gets 403.
- **Capability** `app-windows.json`, for `["image-preview", "float-*"]`, grants only
  `core:event:allow-listen` and `core:event:allow-unlisten`: no `emit`, `emit_to` or
  `core:window:*`. Secondary windows hear only what Rust sends them, and Rust decides their geometry,
  visibility and focus. The glob gives an unregistered `float-*` window events only, never a
  command or an image.
- **Events:** Rust never broadcasts with `app.emit`; every event names its window with `emit_to`, so a
  secondary window hears only its own.

**Consequences:**

- A new secondary window means a label rule in `main_window::is_secondary`, commands in the
  secondary list that check their label, and its own events; `page_ipc.rs` tests the barrier and the
  capability with stand-in labels.
- A secondary window cannot move, show or close itself, nor talk to `main` directly: anything it
  needs goes through a secondary command, and anything `main` needs goes through Rust.
- The image preview never takes focus or clicks: Rust shows it without activating and it stays
  disabled and click-through, so the panel's hover and focus rules ignore it.
