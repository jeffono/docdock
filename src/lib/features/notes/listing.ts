import type { Listing, Note, PinnedItem } from "../../ipc";

/**
 * Whether `path` is one of the `skipped` folders or lies below one. Both come from the disk, so case is
 * ignored as Windows does.
 */
export function inSkipped(path: string, skipped: string[]): boolean {
  const lower = path.toLowerCase();
  return skipped.some((folder) => {
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
  return inSkipped(path, listing.skipped) ? "unread" : "gone";
}

/**
 * The notes to show after `listing`: the listed ones, and the `known` ones it left out only because
 * their folder could not be read, as they were. A known note left out of a folder that was read is
 * gone. A later listing that reads the folder decides again.
 */
export function withUnread(known: Note[], listing: Listing): Note[] {
  if (!listing.skipped.length) return listing.notes;
  const listed = new Set(listing.notes.map((note) => note.path));
  return [
    ...listing.notes,
    ...known.filter((note) => !listed.has(note.path) && inSkipped(note.path, listing.skipped)),
  ];
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
    if (item.kind !== "note" || !item.missing || !inSkipped(item.target, skipped)) return [item];
    return paths.has(item.target) ? [{ ...item, missing: false }] : [];
  });
}
