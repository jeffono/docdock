// The only point of contact between the UI and the Rust backend.
import { Channel, invoke } from "@tauri-apps/api/core";
import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

/** Native exit waits indefinitely for the trusted UI to confirm its saves. */
export const onExitRequested = (handler: (id: number) => void) =>
  listen<number>("exit-requested", (event) => handler(event.payload));
export const onExitCancelled = (handler: () => void) => listen("exit-cancelled", handler);
export const exitReady = (id: number, saved: boolean) => invoke<void>("exit_ready", { id, saved });

export type UpdateStatus = {
  version: string;
  phase:
    | "disabled"
    | "notChecked"
    | "checking"
    | "upToDate"
    | "downloading"
    | "ready"
    | "checkFailed"
    | "downloadFailed"
    | "installFailed";
  /** Timestamp of the most recent successful release check in this session, as epoch ms. */
  lastSuccess: number | null;
};
export const updateStatus = () => invoke<UpdateStatus>("update_status");
export const updateCheck = () => invoke<void>("update_check");
export const updateRestart = () => invoke<void>("update_restart");
export const onUpdateState = (handler: (status: UpdateStatus) => void) =>
  listen<UpdateStatus>("update-state", (event) => handler(event.payload));

export type Locale = "en" | "pt-BR";
export const appLocale = () => invoke<Locale>("app_locale");

export type Theme = "system" | "light" | "dark";
export const appTheme = () => invoke<Theme>("app_theme");
export const appSetTheme = (theme: Theme) => invoke<void>("app_set_theme", { theme });

/** Windows registry state, shared with the tray; never stored in settings.json. */
export const appAutostart = () => invoke<boolean>("app_autostart");
export const appSetAutostart = (enabled: boolean) => invoke<void>("app_set_autostart", { enabled });
export const onAutostartChanged = (handler: (enabled: boolean) => void) =>
  listen<boolean>("autostart-changed", (event) => handler(event.payload));

/**
 * The activation shortcut as shown, such as `Ctrl+Alt+Space`; `failed` could not be registered;
 * `recording`: released while the panel records a new one.
 */
export type ActivationStatus = {
  shortcut: string | null;
  failed: string | null;
  recording: boolean;
};
/** `code` is the physical key, as `KeyboardEvent.code`; Rust reads it through the layout. */
export type Combination = {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  win: boolean;
  code: string;
};
/**
 * While recording, the current shortcut is released so the panel receives its keys. `false` also
 * reads the state. Rust ends a recording itself when the panel loses focus or hides, or after a
 * timeout, and then emits `onActivation`.
 */
export const shortcutRecord = (recording: boolean) =>
  invoke<ActivationStatus>("shortcut_record", { recording });
/** Ends recording. If `combination` is refused or taken, the previous shortcut stays. */
export const shortcutSet = (combination: Combination) =>
  invoke<ActivationStatus>("shortcut_set", { combination });
export const onActivation = (handler: (status: ActivationStatus) => void) =>
  listen<ActivationStatus>("activation", (event) => handler(event.payload));

export function panelActivate(): Promise<void> {
  return invoke("panel_activate");
}

export function panelSetPinned(pinned: boolean): Promise<void> {
  return invoke("panel_set_pinned", { pinned });
}

/** Closes a focused or cursor-opened panel; a pinned one stays shown and returns focus to the previous app. */
export function panelClose(): Promise<void> {
  return invoke("panel_close");
}

export const panelSetSide = (side: PanelFocus["side"]) => invoke<void>("panel_set_side", { side });
/** Logical width, `null` for the default; Rust clamps it to the monitor. `done` ends a drag and saves. */
export const panelResize = (width: number | null, done: boolean) =>
  invoke<void>("panel_resize", {
    width: width === null ? null : Math.max(0, Math.round(width)),
    done,
  });

/** The edge tab's size in logical pixels, which the window's region follows. */
export type TabSize = { depth: number; height: number; curve: number };
/** The tab's size and the system drag tolerance each way in physical pixels. */
export type TabShape = TabSize & { drag: [number, number] };
export const panelTabShape = () => invoke<TabShape>("panel_tab_shape");
/** Rust reshaped the window for a new tab size, which only the tweak window changes. */
export const onPanelTabShape = (handler: (size: TabSize) => void): Promise<UnlistenFn> =>
  listen<TabSize>("panel-tab-shape", (event) => handler(event.payload));
/** The edge tab was clicked; Rust takes it only from the open panel at rest. */
export const panelTab = () => invoke<void>("panel_tab");

/** How a slide moves: milliseconds and `cubic-bezier` control points `[x1, y1, x2, y2]`. */
export type Curve = [number, number, number, number];
export type Motion = { duration: number; curve: Curve };
export type Motions = { open: Motion; close: Motion; handleIn: Motion; handleOut: Motion };
/**
 * The edge handle's activation stretch and reveal radius (percent of the tab height) and the speed
 * toward the edge (logical px/s) that opens the panel at once.
 */
export type HandleTuning = { activationPercent: number; revealPercent: number; rushSpeed: number };

// Development builds only: release builds have neither the commands nor the tweak window.
export const devMotion = (motions: Motions) => invoke<void>("dev_motion", { motions });
export const devHandle = (tuning: HandleTuning) => invoke<void>("dev_handle", tuning);
/** Rust validates the size and answers with `panel-tab-shape` once the window follows it. */
export const devTabShape = (size: TabSize) => invoke<void>("dev_tab_shape", size);
export const devFonts = () => invoke<string[]>("dev_fonts");
export const devTweakWindow = () => invoke<void>("dev_tweak_window");
export const isTweakWindow = () => getCurrentWebviewWindow().label === "tweak";
/** The tweak window sends its whole state to the panel. */
export const tweakSend = (state: unknown) => emitTo("main", "tweak-state", state);
export const onTweakState = <T>(handler: (state: T) => void) =>
  listen<T>("tweak-state", (event) => handler(event.payload));
/** The panel, after a reload, asks the tweak window for its state. */
export const tweakRequest = () => emitTo("tweak", "tweak-request");
export const onTweakRequest = (handler: () => void) => listen("tweak-request", () => handler());
/** The tweak window cannot invoke `dev_fonts`: it asks the panel, which answers with the list. */
type TweakFonts = { families: string[] } | { error: string };
export const tweakFontsRequest = () => emitTo("main", "tweak-fonts-request");
export const onTweakFontsRequest = (handler: () => void) =>
  listen("tweak-fonts-request", () => handler());
export const tweakFontsSend = (fonts: TweakFonts) => emitTo("tweak", "tweak-fonts", fonts);
export const onTweakFonts = (handler: (fonts: TweakFonts) => void) =>
  listen<TweakFonts>("tweak-fonts", (event) => handler(event.payload));

/** Clicking the edge tab closed the pinned panel, unpinning it. */
export const onPanelPinned = (handler: (pinned: boolean) => void): Promise<UnlistenFn> =>
  listen<boolean>("panel-pinned", (event) => handler(event.payload));

/** Whether the panel window or its page is foreground; `side` is the screen edge it docks to. */
export type PanelFocus = { focused: boolean; side: "left" | "right" };
export const onPanelFocus = (handler: (focus: PanelFocus) => void): Promise<UnlistenFn> =>
  listen<PanelFocus>("panel-focus", (event) => handler(event.payload));

/**
 * The shortcut asks for the search bar. `toggle`: the panel window already had focus before it, so
 * if the bar already has the focus the shortcut closes the panel instead.
 */
export const onPanelSearch = (handler: (toggle: boolean) => void): Promise<UnlistenFn> =>
  listen<{ toggle: boolean }>("panel-search", (event) => handler(event.payload.toggle));

export function webOpen(url: string): Promise<void> {
  return invoke("web_open", { url });
}

export const webSetVisible = (visible: boolean) => invoke<void>("web_set_visible", { visible });
export const webHistory = (back: boolean) => invoke<void>("web_history", { back });
/** The shown page, if any. `selection` is the main frame's plain text, empty when there is none. */
export type PageSelection = { url: string; title: string; selection: string };
export const webSelection = () => invoke<PageSelection | null>("web_selection");
/** The shown page's article. `markdown` is untrusted: render it only through `render.ts`. */
export type PageArticle = { url: string; title: string; markdown: string };
export const webReader = () => invoke<PageArticle | null>("web_reader");
/** One document and URL of the page window, as Rust gave it: pass it back unchanged. */
export type PageRevision = { identity: number; sources: number };
/**
 * A screenshot of the shown page drawn by the browser engine: the visible area, or with `full` the
 * whole page, if it still shows `page`. `image` is a PNG, or a JPEG when a PNG would exceed the
 * attachment limit; `url` is the address of the document it shows and `title` a label from about
 * then, both untrusted. `clipped`: the page was too large and only its start was captured. `shown`:
 * for `webCaptureShown`. `folder` and `folderId`: the notes folder it started in, which
 * `notesCreateWithImage` requires.
 */
export type PageCapture = {
  url: string;
  title: string;
  image: Uint8Array;
  clipped: boolean;
  shown: number;
  folder: string;
  folderId: string;
};
export async function webCapture(full: boolean, page: PageRevision): Promise<PageCapture> {
  // A raw body: a little-endian u32 with the length of the page's JSON, that JSON, then the image.
  const body = new Uint8Array(await invoke<ArrayBuffer>("web_capture", { full, page }));
  const length = new DataView(body.buffer, body.byteOffset).getUint32(0, true);
  const shot = JSON.parse(new TextDecoder().decode(body.subarray(4, 4 + length))) as Omit<
    PageCapture,
    "image"
  >;
  return { ...shot, image: body.slice(4 + length) };
}
/** The panel still shows `page`, as it did when the capture that returned `shown` started. */
export const webCaptureShown = (page: PageRevision, shown: number) =>
  invoke<boolean>("web_capture_shown", { page, shown });
/** Whether the page's context menu enables its capture items. */
export const webOfferCapture = (offered: boolean) => invoke<void>("web_offer_capture", { offered });
/**
 * A capture item chosen in the page's native context menu: `full` page or the visible area, and
 * the `page` revision the menu opened on.
 */
export type CaptureChoice = { full: boolean; page: PageRevision };
export const onWebCapture = (handler: (choice: CaptureChoice) => void) =>
  listen<CaptureChoice>("web-capture", (event) => handler(event.payload));
export type PageState = { url: string; title: string; canGoBack: boolean; canGoForward: boolean };
export const onWebState = (handler: (state: PageState | null) => void) =>
  listen<PageState | null>("web-state", (event) => handler(event.payload));
export const onWebVisible = (handler: (visible: boolean) => void) =>
  listen<boolean>("web-visible", (event) => handler(event.payload));

/** Ctrl+Tab / Ctrl+Shift+Tab typed while the page window had focus, filtered natively before the page sees it. */
export const onToggleMode = (handler: (step: 1 | -1) => void) =>
  listen<number>("panel-toggle-mode", (event) => handler(event.payload < 0 ? -1 : 1));

/**
 * `unavailable`: its drive, share or a folder above it is gone, as for a locked vault. `missing`: the
 * folder above it is there but the folder is not. Neither is ever created. `unrecognized`: a folder is
 * there but its identity marker is not the chosen one's, as for the empty folder a locked vault can
 * leave at its mount point; it is never written to.
 */
export type FolderState = "available" | "unavailable" | "missing" | "unrecognized";
export type NotesFolder = { path: string; state: FolderState };
/** `modified` is in ms since the Unix epoch; 0 if unknown. */
export type Note = { path: string; name: string; modified: number; created: number };
export const notesFolder = () => invoke<NotesFolder | null>("notes_folder");
/** Only the saved, validated Cryptomator file can be opened; no launch path comes from the UI. */
export const vaultFile = () => invoke<string | null>("vault_file");
export const vaultChoose = () => invoke<string | null>("vault_choose");
export const vaultClear = () => invoke<void>("vault_clear");
export const vaultUnlock = () => invoke<void>("vault_unlock");
export const notesChoose = () => invoke<NotesFolder | null>("notes_choose");
/** Confirms the folder at the configured path, as choosing it again would; an open editor stays. */
export const notesTrust = () => invoke<NotesFolder | null>("notes_trust");
/**
 * `skipped`: subfolders that could not be read, relative with `/`. Notes in them may be left out, or
 * only some listed: their absence does not mean they are gone.
 */
export type Listing = { notes: Note[]; skipped: string[] };
export const notesList = () => invoke<Listing>("notes_list");
/** `version` is opaque: pass it back to `notesSave` as `base`. */
export type OpenNote = {
  text: string;
  version: string;
};
export type SaveResult = { status: "saved"; version: string } | { status: "conflict" };
export const notesOpen = (path: string) => invoke<OpenNote>("notes_open", { path });
/** Raw texts of many notes in one call, for the search index. Unreadable notes are left out. */
type NoteText = { path: string; text: string; modified: number };
export const notesTexts = (paths: string[]) => invoke<NoteText[]>("notes_texts", { paths });
export const notesSave = (path: string, content: string, base: string) =>
  invoke<SaveResult>("notes_save", { path, content, base });
/** As `notesSave`, but a note that is gone answers `conflict` instead of being recreated. */
export const notesRewrite = (path: string, content: string, base: string) =>
  invoke<SaveResult>("notes_rewrite", { path, content, base });
/**
 * Renames a note within its folder if it still holds `base`, never over another file: fails with
 * `renameConflict` otherwise. Pinned items follow; `title` is the old and new title, for their link
 * text. `pinnedNotRenamed`: the note moved but its pinned items did not.
 */
export const notesRename = (from: string, to: string, base: string, title: [string, string]) =>
  invoke<void>("notes_rename", { from, to, base, title });
/**
 * Moves a note to `to` in another existing folder, as `notesRename` does, keeping the link text of its
 * pinned items. Hidden folders and the note's own folder are refused.
 */
export const notesMove = (from: string, to: string, base: string) =>
  invoke<void>("notes_move", { from, to, base });
/** Every folder listing visits, as `/` paths; the root is not included. */
export const notesFolders = () => invoke<{ path: string; empty: boolean }[]>("notes_folders");
/** Creates a folder, nested with `/`, keeping one that exists; `invalidFolderName` otherwise. */
export const notesCreateFolder = (path: string) => invoke<void>("notes_create_folder", { path });
/**
 * Where a deleted note or folder went. `undo`: `notesUndoDelete` can bring the note back from the
 * Recycle Bin until the panel closes. `trash`: the Recycle Bin could not take it, as on an encrypted
 * vault or a network folder, so it went to the trash inside the notes folder for 30 days, as this
 * entry, which `notesTrashRestore` brings back. `images`: how many images only the note referenced
 * follow it the same way after the panel hides (`notesSendPending`); an Undo before then needs
 * nothing for them.
 */
export type Removal = { undo: boolean; trash: string | null; images: number };
/**
 * Deletes a folder and everything in it at once. Its notes' pinned items are removed;
 * `pinnedNotRemoved` (with the `Removal` fields): the folder was deleted all the same.
 */
export const notesDeleteFolder = (path: string) => invoke<Removal>("notes_delete_folder", { path });
/**
 * Deletes a note and removes its pinned items. While `notesUndoDelete` can bring it back, saves or
 * renames to its path answer a conflict. `pinnedNotRemoved` (with the `Removal` fields): the note was
 * deleted all the same.
 */
export const notesDelete = (path: string) => invoke<Removal>("notes_delete", { path });
/** `restoreTaken`: a file took the name; nothing changed and undoing stays possible. */
export const notesUndoDelete = (path: string) => invoke<void>("notes_undo_delete", { path });
/**
 * Once the panel hid and the editor it left saved: the images of the notes deleted in ended panel
 * sessions follow them, if no note uses them by then (`Removal.images`).
 */
export const notesSendPending = (session: number) =>
  invoke<void>("notes_send_pending", { session });
/**
 * An entry of the trash inside the notes folder; `deleted` is in ms since the Unix epoch. `path` is
 * null when its item cannot be told, as when a sync conflict left two items in it: it can only be
 * emptied. An entry left with only images a note's restore could not move names the first of them
 * and restores them all.
 */
export type TrashItem = { id: string; path: string | null; folder: boolean; deleted: number };
/** Newest first. */
export const notesTrash = () => invoke<TrashItem[]>("notes_trash");
/**
 * Moves an item back to its path, or a free name such as "name (2).md" when that is taken, and
 * resolves to the path it took; the images deleted with a note return to their own paths with it.
 * `undo` also puts back the pinned items its delete removed. `trashItemMissing`: it is no longer in
 * the trash. `imagesLeftInTrash`: the item is back at `path`, but `count` images, whose names are
 * taken or which could not move, stay in the trash, as one item.
 */
export const notesTrashRestore = (id: string, undo = false) =>
  invoke<string>("notes_trash_restore", { id, undo });
/** Deletes the entries `ids`, the ones listed, for good; anything synced in since stays. */
export const notesTrashEmpty = (ids: string[]) => invoke<void>("notes_trash_empty", { ids });

export type UnusedImages = { folderId: string; paths: string[] };
export const notesUnusedImages = () => invoke<UnusedImages>("notes_unused_images");
export const notesRecycleImages = (folderId: string, paths: string[]) =>
  invoke<void>("notes_recycle_images", { folderId, paths });
/**
 * Where a note made from a pasted image goes, for `notesCreateWithImage`, and `title`, what the
 * window it came from suggests naming it, if anything (untrusted).
 */
export type PasteTarget = { folder: string; folderId: string; title: string | null };
/** `dropped`: for something dragged in, named after the window the panel opened over. */
export const notesPasteTarget = (dropped = false) =>
  invoke<PasteTarget>("notes_paste_target", { dropped });
export const notesPasteImage = (path: string, bytes: Uint8Array) =>
  invoke<string>("notes_paste_image", bytes, {
    headers: { "x-note-path": encodeURIComponent(path) },
  });
/**
 * Creates the note `path` holding `content` and then a link to `image`, put beside it as a pasted
 * image is, in one write: `conflict` if the name is taken, before the image is written. Fails with
 * `folderChanged` unless the notes folder is still `folder` with the marker id `folderId`, and with
 * `imageKept` if the note could not be created after the image was.
 */
export function notesCreateWithImage(
  folder: string,
  folderId: string,
  path: string,
  content: string,
  image: Uint8Array,
) {
  // A raw body: a little-endian u32 with the length of `content` in UTF-8, `content`, the image.
  const text = new TextEncoder().encode(content);
  const body = new Uint8Array(4 + text.length + image.length);
  new DataView(body.buffer).setUint32(0, text.length, true);
  body.set(text, 4);
  body.set(image, 4 + text.length);
  return invoke<SaveResult>("notes_create_with_image", body, {
    headers: {
      "x-note-path": encodeURIComponent(path),
      "x-notes-folder": encodeURIComponent(folder),
      "x-notes-folder-id": encodeURIComponent(folderId),
    },
  });
}

/**
 * A pinned item of `_pinned.md`, in file order. `target` is a note's path or an http(s) URL and is the key
 * for every pinned call. `missing`: the note is gone, so offer to remove it. `collapsed`: show one line.
 */
export type PinnedItem = {
  target: string;
  title: string;
  kind: "note" | "url";
  missing: boolean;
  collapsed: boolean;
};
/** `restorable`: `_pinned.md` was deleted and `pinnedRestore` can bring it back. */
export type Pinned = { version: string; items: PinnedItem[]; restorable: boolean };
/** Every pinned call resolves to the state after the change. Reread on `onNotesChanged`. */
export const pinnedList = () => invoke<Pinned>("pinned_list");
export const pinnedAdd = (target: string, title: string) =>
  invoke<Pinned>("pinned_add", { target, title });
export const pinnedRemove = (target: string) => invoke<Pinned>("pinned_remove", { target });
export const pinnedSetCollapsed = (target: string, collapsed: boolean) =>
  invoke<Pinned>("pinned_set_collapsed", { target, collapsed });
export const pinnedRestore = () => invoke<Pinned>("pinned_restore");

export const openExternal = (url: string) => invoke<void>("open_external", { url });

/** Replaces the clipboard with text kept out of Windows clipboard history and cloud clipboard. */
export const clipboardWrite = (text: string) => invoke<void>("clipboard_write", { text });

/** Something in the notes folder changed outside the app, or a save landed. */
export type NotesChanges = { renames: [string, string][] };
export const onNotesChanged = (handler: (changes: NotesChanges) => void): Promise<UnlistenFn> =>
  listen<NotesChanges>("notes-changed", (event) => handler(event.payload));

/** A post-it asked to edit its note: the panel took focus and opens it in the editor. */
/**
 * A post-it asks to edit its note `path`, from the notes folder of floating windows' `epoch`:
 * `floatingEditCurrent` tells whether that folder is still the configured one.
 */
export type EditNoteRequest = { path: string; epoch: number };
export const onPanelEditNote = (handler: (request: EditNoteRequest) => void): Promise<UnlistenFn> =>
  listen<EditNoteRequest>("panel-edit-note", (event) => handler(event.payload));

/** The panel finished sliding out of sight, ending the panel session it carries. */
export const onPanelHidden = (handler: (session: number) => void): Promise<UnlistenFn> =>
  listen<number>("panel-hidden", (event) => handler(event.payload));

/** A backup an interrupted save left next to `note`; `modified` is in ms since the Unix epoch. */
export type Leftover = { path: string; note: string; modified: number };
/**
 * The side image preview, a window beside the panel. `rect` is the thumbnail's box in this
 * viewport as `[x, y, width, height]` CSS pixels; `natural` is the image's size in pixels. Rust
 * hides it on its own, as when the panel hides, and then emits `image-preview-closed`.
 */
export const imagePreviewShow = (
  path: string,
  rect: [number, number, number, number],
  natural: [number, number],
) => invoke<void>("image_preview_show", { path, rect, natural });
export const imagePreviewShowNote = (path: string, rect: [number, number, number, number]) =>
  invoke<void>("image_preview_show_note", { path, rect });
export const imagePreviewHide = () => invoke<void>("image_preview_hide");
export const onImagePreviewClosed = (handler: () => void): Promise<UnlistenFn> =>
  listen("image-preview-closed", () => handler());

/**
 * In the preview window itself, which has no capability: it reaches only its own commands, which
 * answer what to show now and send each later change through `onUpdate`. The highest `version` is
 * the latest, whichever way it arrives.
 */
export const isImagePreviewWindow = () => getCurrentWebviewWindow().label === "image-preview";
/** `side` is the panel's, which the frame's arrow faces; `arrowY` its tip in CSS pixels. */
export type ImagePreviewUpdate = {
  version: number;
  kind: "image" | "note" | "postit";
  path: string | null;
  side: "left" | "right";
  arrowY: number;
};
export function imagePreviewState(onUpdate: (update: ImagePreviewUpdate) => void) {
  const channel = new Channel<ImagePreviewUpdate>();
  channel.onmessage = onUpdate;
  return invoke<ImagePreviewUpdate & { locale: Locale }>("image_preview_state", { channel });
}
/** Reads the closed note currently shown in the side preview window, or `null` once hidden/gone. */
export const imagePreviewNote = () => invoke<string | null>("image_preview_note");

/**
 * Detaches the image at `path` into a floating window, opened where the side preview would show
 * it for its thumbnail at `rect` (`[x, y, width, height]` CSS pixels in this viewport); `natural`
 * is the image's size in pixels, `[0, 0]` if unknown.
 */
export const floatingOpen = (
  path: string,
  rect: [number, number, number, number],
  natural: [number, number],
) => invoke<void>("floating_open", { path, rect, natural });

/** Opens the note at `path` as a post-it beside its card at `rect`, as `floatingOpen` places it. */
export const floatingOpenNote = (path: string, rect: [number, number, number, number]) =>
  invoke<void>("floating_open_note", { path, rect });
/** Starts a live pointer drag for `path`, showing its post-it ghost or moving its open window. */
export const floatingDragStart = (id: number, path: string) =>
  invoke<void>("floating_drag_start", { id, path });
/** Finishes the active note drag; `commit` opens or moves the post-it when outside the panel. */
export const floatingDragEnd = (
  id: number,
  commit: boolean,
  cursor: [number, number] | null = null,
) => invoke<boolean>("floating_drag_end", { id, commit, cursor });
/** Emitted if Rust cancels an active note drag (Esc, right click, dialog or panel hide). */
export const onPanelDragCancel = (handler: (id: number) => void): Promise<UnlistenFn> =>
  listen<number>("panel-drag-cancel", (event) => handler(event.payload));
/** No folder change came since floating windows' `epoch`, nor is one under way. */
export const floatingEditCurrent = (epoch: number) =>
  invoke<boolean>("floating_edit_current", { epoch });

/** The images and notes open floating windows show, relative to the notes folder. */
export const floatingList = () => invoke<string[]>("floating_list");
/** Closes the floating windows showing `path`, an image or a note. */
export const floatingClosePath = (path: string) => invoke<void>("floating_close_path", { path });
/** Rust tells `main` whenever the images and notes open floating windows show change. */
export const onFloatingPaths = (handler: (paths: string[]) => void): Promise<UnlistenFn> =>
  listen<string[]>("floating-paths", (event) => handler(event.payload));
/**
 * The panel edits the note at `path`, or none (`null`): a note is edited in one place at a time,
 * so a post-it editing it saves and returns to reading first; `false` if it could not, or while
 * edits are held (exit, folder choice).
 */
export const floatingPanelEdit = (path: string | null) =>
  invoke<boolean>("floating_panel_edit", { path });
/**
 * A post-it asks to edit the note the panel edits, `path`: the panel leaves its editor, saving,
 * and answers `request` with `floatingPanelReleased` (`false` if it could not).
 */
export const onFloatingReleasePanelEdit = (
  handler: (release: { request: number; path: string }) => void,
): Promise<UnlistenFn> =>
  listen<{ request: number; path: string }>("floating-release-panel-edit", (event) =>
    handler(event.payload),
  );
export const floatingPanelReleased = (request: number, released: boolean) =>
  invoke<void>("floating_panel_released", { request, released });
/**
 * Before exiting or choosing another notes folder: holds new edits and has every post-it editing
 * save and return to reading. `false` if one could not, or is parked; then call
 * `floatingReleaseEdits` unless the exit or folder choice goes on.
 */
export const floatingFinishEdits = () => invoke<boolean>("floating_finish_edits");
/** Edits may begin again: the exit or folder choice `floatingFinishEdits` held them for ended. */
export const floatingReleaseEdits = () => invoke<void>("floating_release_edits");
/** Rust tells `main` whenever the notes post-its edit change. */
export const onFloatingEditing = (handler: (paths: string[]) => void): Promise<UnlistenFn> =>
  listen<string[]>("floating-editing", (event) => handler(event.payload));

/**
 * In a floating window itself, which has no capability: it reaches only the `float_*` commands,
 * each acting on itself. Rust sends each change of what it shows through `onUpdate`; the highest
 * `version` is the latest, whichever way it arrives.
 */
export const isFloatWindow = () => /^float-[1-9]\d*$/.test(getCurrentWebviewWindow().label);
export type FloatUpdate = {
  /** Changes on Escape over a post-it, even while another window has focus. */
  escapeRevision: number;
  version: number;
  /** A floating image, or a post-it showing a note. */
  kind: "image" | "note";
  path: string | null;
  /** Changes whenever a post-it's note may have changed: read it again with `floatNote`. */
  revision: number;
  /** From 0.15 to 1. */
  opacity: number;
  /** A post-it's text size against the panel's, from 0.5 to 3. */
  textScale: number;
  clickThrough: boolean;
  /** Alt is held over the click-through window: it takes clicks and shows its controls. */
  revealed: boolean;
  /** The post-it shows only its title bar. */
  collapsed: boolean;
  /** The post-it edits its note (`floatBeginEdit`). */
  editing: boolean;
  /** The post-it's note is edited elsewhere, in the panel or another post-it. */
  blocked: boolean;
  /**
   * The notes folder left, or another was chosen, while the post-it edited: it must hide the note
   * at once but keep its text, until the same folder returns (`false` again, with a new
   * `revision`) or the user discards it (`floatEndEdit`, which closes the window keeping its
   * place). Saving fails meanwhile.
   */
  parked: boolean;
  /**
   * Rust asks the editing post-it to finish: on each new value, save, then answer it with
   * `floatFinished(finish, saved)`; a save ends the edit as `floatEndEdit` does, a failure (as a
   * conflict) keeps editing. 0 before any request.
   */
  finish: number;
  /**
   * The user closed the editing post-it from outside it (Alt+F4, the panel, the tray): on each new
   * value, close as the post-it's own close button would, saving first and staying open if that
   * fails. 0 before any request.
   */
  closeRequested: number;
};
export function floatState(onUpdate: (update: FloatUpdate) => void) {
  const channel = new Channel<FloatUpdate>();
  channel.onmessage = onUpdate;
  return invoke<FloatUpdate & { locale: Locale }>("float_state", { channel });
}
export const floatSetOpacity = (opacity: number) => invoke<void>("float_set_opacity", { opacity });
export const floatSetClickThrough = (clickThrough: boolean) =>
  invoke<void>("float_set_click_through", { clickThrough });
/**
 * Closes the window and forgets its place. An editing post-it calls it only once nothing is left
 * to save (or the user discarded): its edit ends first.
 */
export const floatClose = () => invoke<void>("float_close");
/** The image's size after a zoom, in CSS pixels, and whether it reached its largest. */
export type FloatZoomed = { width: number; height: number; atMax: boolean };
/**
 * Grows or shrinks the window so its image is `width` CSS pixels wide, keeping its proportions,
 * around its center, up to the side preview's share of the monitor.
 */
export const floatZoom = (width: number) => invoke<FloatZoomed>("float_zoom", { width });
/** The image's natural size, so the window keeps the image's own proportions. */
export const floatNatural = (width: number, height: number) =>
  invoke<void>("float_natural", { width, height });
/** The note a post-it shows, read now; `null` once it is gone. */
export const floatNote = () => invoke<OpenNote | null>("float_note");
/** A note a post-it began editing, and the edit granted, which its saves name. */
export type FloatGranted = OpenNote & { lease: number };
/**
 * The post-it begins editing its note and takes focus: the note to edit, or `null` if it may not
 * now, as while the note is edited elsewhere (`blocked`) or clicks go through the post-it.
 */
export const floatBeginEdit = () => invoke<FloatGranted | null>("float_begin_edit");
/**
 * Saves the note the post-it edits, never a path it names, if it still holds `base` and the edit
 * `lease` (from `floatBeginEdit`) lasts: a save from an edit that ended is refused. A note that is
 * gone answers `conflict` unless `recreate`.
 */
export const floatSaveNote = (content: string, base: string, lease: number, recreate = false) =>
  invoke<SaveResult>("float_save_note", { content, base, lease, recreate });
/**
 * The post-it stops editing and gives focus back to the window that had it. A parked post-it then
 * closes, keeping its place for when its folder returns: call it once nothing is left to save, or
 * when the user discards.
 */
export const floatEndEdit = () => invoke<void>("float_end_edit");
/** Answers Rust's finish request `request` (`FloatUpdate.finish`): the post-it saved, or not. */
export const floatFinished = (request: number, saved: boolean) =>
  invoke<void>("float_finished", { request, saved });
export const floatSetTextScale = (scale: number) => invoke<void>("float_set_text_scale", { scale });
/** Collapses a post-it to its title bar, keeping its top, or expands it back. */
export const floatSetCollapsed = (collapsed: boolean) =>
  invoke<void>("float_set_collapsed", { collapsed });
/** Opens a post-it's note in the panel for editing. */
export const floatEdit = () => invoke<void>("float_edit");
/** Moves the window with the pointer; call while the primary button is down. */
export const floatDrag = () => invoke<void>("float_drag");
/** Signals that a newly created post-it painted its first frame so its drag ghost can hide. */
export const floatReady = () => invoke<void>("float_ready");
export type ResizeDirection =
  | "North"
  | "South"
  | "East"
  | "West"
  | "NorthEast"
  | "NorthWest"
  | "SouthEast"
  | "SouthWest";
/** Resizes the window from an edge or corner with the pointer, as a frame would. */
export const floatResize = (direction: ResizeDirection) =>
  invoke<void>("float_resize", { direction });

export const notesLeftovers = () => invoke<Leftover[]>("notes_leftovers");
/** Resolves to the path of the restored note. */
export const notesRestore = (path: string) => invoke<string>("notes_restore", { path });

export const searchEngines = [
  "duckduckgo",
  "brave",
  "startpage",
  "ecosia",
  "google",
  "bing",
  "custom",
] as const;
export type SearchEngine = (typeof searchEngines)[number];
export type SearchPreferences = { engine: SearchEngine; customUrl: string; invalidCustom: boolean };

export const webSearchPreferences = () => invoke<SearchPreferences>("web_search_preferences");
export const webSetSearchPreferences = (engine: SearchEngine, customUrl: string) =>
  invoke<SearchPreferences>("web_set_search_preferences", { engine, customUrl });
export const webSearchUrl = (query: string) => invoke<string>("web_search_url", { query });
export const webSuggestions = (query: string) => invoke<string[]>("web_suggestions", { query });
export const webAddresses = () => invoke<string[]>("web_addresses");
export const webOverlay = (height: number) => invoke<void>("web_overlay", { height });

export const noteOrders = [
  "modified-desc",
  "modified-asc",
  "name-asc",
  "name-desc",
  "created-desc",
  "created-asc",
] as const;
export type NoteOrder = (typeof noteOrders)[number];
export const notesOrder = () => invoke<NoteOrder>("notes_order");
export const notesSetOrder = (order: NoteOrder) => invoke<void>("notes_set_order", { order });
export const notesRelativeDates = () => invoke<boolean>("notes_relative_dates");
export const notesSetRelativeDates = (enabled: boolean) =>
  invoke<void>("notes_set_relative_dates", { enabled });
export type Grouping = "none" | "date" | "folder";
export const notesGrouping = () => invoke<Grouping>("notes_grouping");
export const notesSetGrouping = (grouping: Grouping) =>
  invoke<void>("notes_set_grouping", { grouping });
export const pinnedMove = (target: string, to: string, base: string) =>
  invoke<Pinned>("pinned_move", { target, to, base });

export type BlockingStatus = { site: string; allowed: boolean };
export const blockingStatus = (url: string) =>
  invoke<BlockingStatus | null>("blocking_status", { url });
export const blockingSites = () => invoke<string[]>("blocking_sites");
export const blockingSetAllowed = (url: string, allowed: boolean) =>
  invoke<void>("blocking_set_allowed", { url, allowed });

export type PermissionKind = "location" | "camera" | "microphone" | "clipboardRead";
/**
 * A site's request the panel asks about; the panel stays open until it is answered. `site` decides
 * and is remembered; `origin`: the asking frame's, only when it is not the page's.
 */
export type PermissionPrompt = {
  id: number;
  site: string;
  kind: PermissionKind;
  origin: string | null;
};
/** The request to show, or null when none waits. */
export const onPermission = (handler: (prompt: PermissionPrompt | null) => void) =>
  listen<PermissionPrompt | null>("permission", (event) => handler(event.payload));
/** Answers the request and remembers the choice for its site and kind. */
export const permissionAnswer = (id: number, allow: boolean) =>
  invoke<void>("permission_answer", { id, allow });
export type SitePermission = { site: string; kind: PermissionKind; decision: "allow" | "block" };
export const permissionList = () => invoke<SitePermission[]>("permission_list");
/** Forgets a choice: the site asks again. */
export const permissionForget = (site: string, kind: PermissionKind) =>
  invoke<void>("permission_forget", { site, kind });

export const noteSearchSuggestionsEnabled = () => invoke<boolean>("note_search_suggestions");
export const noteSearchSetSuggestions = (enabled: boolean) =>
  invoke<void>("note_search_set_suggestions", { enabled });

export const notePreviewModes = ["off", "hover", "alt"] as const;
export type NotePreviewMode = (typeof notePreviewModes)[number];
export const notesPreviewMode = () => invoke<NotePreviewMode>("notes_preview_mode");
export const notesSetPreviewMode = (mode: NotePreviewMode) =>
  invoke<void>("notes_set_preview_mode", { mode });
export const tabWheelModes = ["off", "modifier", "on"] as const;
export type TabWheelMode = (typeof tabWheelModes)[number];
export const panelTabWheel = () => invoke<TabWheelMode>("panel_tab_wheel");
export const panelSetTabWheel = (mode: TabWheelMode) =>
  invoke<void>("panel_set_tab_wheel", { mode });
export const scratchpadOpen = () => invoke<OpenNote>("scratchpad_open");
export const scratchpadSave = (content: string, base: string) =>
  invoke<SaveResult>("scratchpad_save", { content, base });
export const onScratchpadChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen("scratchpad-changed", () => handler());

export type ClipboardEntry = {
  id: string;
  text: string;
  truncated: boolean;
  timestampMs: number | null;
  deletable: boolean;
};
export type ClipboardSnapshot = {
  historyEnabled: boolean;
  accessDenied: boolean;
  currentNonText: boolean;
  items: ClipboardEntry[];
};
export const clipboardSnapshot = () => invoke<ClipboardSnapshot>("clipboard_snapshot");
export const clipboardRestoreItem = (id: string) =>
  invoke<boolean>("clipboard_restore_item", { id });
export const clipboardDeleteItem = (id: string) => invoke<boolean>("clipboard_delete_item", { id });
export const clipboardOpenSettings = () => invoke<void>("clipboard_open_settings");
export const onClipboardChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen("clipboard-changed", () => handler());
export const onPanelAlt = (handler: (held: boolean) => void): Promise<UnlistenFn> =>
  listen<boolean>("panel-alt", (event) => handler(event.payload));

/** Spell checking in the editor, on unless turned off in Preferences. */
export const spellingEnabled = () => invoke<boolean>("spelling_enabled");
export const spellingSetEnabled = (enabled: boolean) =>
  invoke<void>("spelling_set_enabled", { enabled });
/** BCP 47 tags of the Windows user languages that have a dictionary, in the user's order. */
export const spellingLanguages = () => invoke<string[]>("spelling_languages");
/** One flag per word, true when every language marks it misspelled. */
export const spellingCheck = (words: string[]) => invoke<boolean[]>("spelling_check", { words });
export const spellingSuggest = (word: string) => invoke<string[]>("spelling_suggest", { word });
/** Adds the word to the Windows dictionary of the first language. */
export const spellingAdd = (word: string) => invoke<void>("spelling_add", { word });
