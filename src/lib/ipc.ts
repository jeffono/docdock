// The only point of contact between the UI and the Rust backend.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export function appVersion(): Promise<string> {
  return invoke<string>("app_version");
}

export type Locale = "en" | "pt-BR";
export const appLocale = () => invoke<Locale>("app_locale");

export type Theme = "system" | "light" | "dark";
export const appTheme = () => invoke<Theme>("app_theme");
export const appSetTheme = (theme: Theme) => invoke<void>("app_set_theme", { theme });

export function panelActivate(): Promise<void> {
  return invoke("panel_activate");
}

export function panelSetPinned(pinned: boolean): Promise<void> {
  return invoke("panel_set_pinned", { pinned });
}

/** Closes a focused or cursor-opened panel; a pinned one stays. */
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
export type PageState = { url: string; title: string; canGoBack: boolean; canGoForward: boolean };
export const onWebState = (handler: (state: PageState | null) => void) =>
  listen<PageState | null>("web-state", (event) => handler(event.payload));
export const onWebVisible = (handler: (visible: boolean) => void) =>
  listen<boolean>("web-visible", (event) => handler(event.payload));

/** Ctrl+Tab typed while the page window had focus, filtered natively before the page sees it. */
export const onToggleMode = (handler: () => void) => listen("panel-toggle-mode", () => handler());

export type NotesFolder = { path: string; available: boolean };
/** `modified` is in ms since the Unix epoch; 0 if unknown. */
export type Note = { path: string; name: string; modified: number; created: number };
export const notesFolder = () => invoke<NotesFolder | null>("notes_folder");
export const notesChoose = () => invoke<NotesFolder | null>("notes_choose");
export const notesList = () => invoke<Note[]>("notes_list");
/** `version` is opaque: pass it back to `notesSave` as `base`. */
export type OpenNote = {
  text: string;
  version: string;
};
export type SaveResult = { status: "saved"; version: string } | { status: "conflict" };
export const notesOpen = (path: string) => invoke<OpenNote>("notes_open", { path });
/** Raw texts of many notes in one call, for the search index. Unreadable notes are left out. */
export type NoteText = { path: string; text: string; modified: number };
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
/** Empty folders go at once; a note count asks for confirmation. null means deleted. */
export const notesDeleteFolder = (path: string, confirmed = false) =>
  invoke<{ notes: number; other: number } | null>("notes_delete_folder", { path, confirmed });
/**
 * Moves a note to the Recycle Bin and removes its pinned items. Resolves to whether, until the panel
 * closes, `notesUndoDelete` can bring both back; meanwhile saves or renames to its path answer a
 * conflict. `notRecyclable`: the note stays where it is. `pinnedNotRemoved` (with `undo`): the note
 * was deleted all the same.
 */
export const notesDelete = (path: string) => invoke<boolean>("notes_delete", { path });
/** `restoreTaken`: a file took the name; nothing changed and undoing stays possible. */
export const notesUndoDelete = (path: string) => invoke<void>("notes_undo_delete", { path });
export const notesPasteImage = (path: string, bytes: Uint8Array) =>
  invoke<string>("notes_paste_image", bytes, {
    headers: { "x-note-path": encodeURIComponent(path) },
  });

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

/** Something in the notes folder changed outside the app, or a save landed. */
export type NotesChanges = { renames: [string, string][] };
export const onNotesChanged = (handler: (changes: NotesChanges) => void): Promise<UnlistenFn> =>
  listen<NotesChanges>("notes-changed", (event) => handler(event.payload));

/** The panel finished sliding out of sight. */
export const onPanelHidden = (handler: () => void): Promise<UnlistenFn> =>
  listen("panel-hidden", handler);

/** A backup an interrupted save left next to `note`; `modified` is in ms since the Unix epoch. */
export type Leftover = { path: string; note: string; modified: number };
export const notesLeftovers = () => invoke<Leftover[]>("notes_leftovers");
/** Resolves to the path of the restored note. */
export const notesRestore = (path: string) => invoke<string>("notes_restore", { path });

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
export type Grouping = "none" | "date" | "folder";
export const notesGrouping = () => invoke<Grouping>("notes_grouping");
export const notesSetGrouping = (grouping: Grouping) =>
  invoke<void>("notes_set_grouping", { grouping });
export const pinnedMove = (target: string, to: string, base: string) =>
  invoke<Pinned>("pinned_move", { target, to, base });
