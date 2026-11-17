import type { Listing, Note, PinnedItem } from "../../ipc";

/**
 * Whether `path` is one of `folders` or lies below one. Paths come from the disk, so case is ignored
 * as Windows does.
 */
export function inAny(path: string, folders: string[]): boolean {
  const lower = path.toLowerCase();
  return folders.some((folder) => {
    const prefix = folder.toLowerCase();
    return lower === prefix || lower.startsWith(`${prefix}/`);
  });
}

/**
 * What a listing says about a note: `listed`; `unread`, left out because its folder could not be read,
 * so it may still exist; or `gone`, left out of a folder that was read.
 */
export function listingState(path: string, listing: Listing): "listed" | "unread" | "gone" {
  if (listing.notes.some((note) => note.path === path)) return "listed";
  return inAny(path, listing.skipped) ? "unread" : "gone";
}

/**
 * The `known` notes that `listing` left out only because their folder could not be read: shown as
 * they were, beside the listed ones. A known note left out of a folder that was read is gone. A later
 * listing that reads the folder decides again.
 */
export function unreadKnown(known: Note[], listing: Listing): Note[] {
  if (!listing.skipped.length) return [];
  const listed = new Set(listing.notes.map((note) => note.path));
  return known.filter((note) => !listed.has(note.path) && inAny(note.path, listing.skipped));
}

/**
 * The known notes after renames or moves, so a note kept from an unreadable folder keeps its new
 * path. Applying a pair again, as a repeated event does, changes nothing.
 */
export function renamedKnown(known: Note[], renames: [string, string][]): Note[] {
  let notes = known;
  for (const [from, to] of renames) {
    if (!notes.some((note) => note.path === from)) continue;
    const taken = notes.some((note) => note.path === to);
    const name = to.slice(to.lastIndexOf("/") + 1).replace(/\.md$/i, "");
    notes = taken
      ? notes.filter((note) => note.path !== from)
      : notes.map((note) => (note.path === from ? { ...note, path: to, name } : note));
  }
  return notes;
}

/**
 * The known notes without a note or folder the app deleted, everything below it included: deleting
 * works where listing does not, so a skipped folder must not keep it.
 */
export function withoutRemoved(known: Note[], path: string): Note[] {
  return known.filter((note) => !inAny(note.path, [path]));
}

/**
 * Pinned items as the list shows them. Rust marks a note it cannot reach as missing, which would offer
 * to remove it; in a skipped folder it is only unread: a known note shows as it was, and an unknown
 * one stays out of sight, still pinned, until its folder can be read.
 */
export function shownPinned(items: PinnedItem[], skipped: string[], known: Note[]): PinnedItem[] {
  if (!skipped.length) return items;
  const paths = new Set(known.map((note) => note.path));
  return items.flatMap((item) => {
    if (item.kind !== "note" || !item.missing || !inAny(item.target, skipped)) return [item];
    return paths.has(item.target) ? [{ ...item, missing: false }] : [];
  });
}

/**
 * Reads until no rename or delete of a known note landed during the read: `version` counts them. A
 * listing taken before one would put the old path back, and a skipped folder would then keep it.
 */
export async function settledRead<T>(read: () => Promise<T>, version: () => number): Promise<T> {
  for (;;) {
    const started = version();
    const result = await read();
    if (version() === started) return result;
  }
}
