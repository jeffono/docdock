// The note list as a pure model: which cards and rows exist, in which order, and what each keyboard
// action does. The component only draws it and runs the effects. No IPC here.
import type { PinnedItem } from "../../ipc";
import type { DateGroup } from "./grouping";
import type { Hit } from "./search";

/** `line` is a one-line row for a note that is not pinned. */
export type Level = "line" | "preview" | "expanded";
export type Card = {
  hit: Hit;
  /** Web cards stay one line and open this URL instead of expanding. */
  url?: string;
  pinned: boolean;
  level: Level;
  /** The note was deleted or moved outside the app; it stays on screen with its last content. */
  gone: boolean;
};
/** A pinned note that no longer exists. */
export type Missing = { target: string; title: string };
export type ListModel = {
  /** The text a "Create" item would use; null when the query is empty or a title already equals it. */
  create: string | null;
  /** In `_pinned.md` order. */
  pinned: (Card | Missing)[];
  /** In search order, without the pinned ones. */
  notes: Card[];
};

export const isMissing = (entry: Card | Missing): entry is Missing => "target" in entry;

export function buildList(input: {
  hits: Hit[];
  exactTitle: boolean;
  query: string;
  pinned: PinnedItem[];
  /** Path of the expanded card, or "". */
  expanded: string;
  /**
   * The expanded note when the hits leave it out, with `gone` when it is no longer listed. A gone note
   * stays on screen with its last content; a note the search filtered out only stays while edited.
   */
  kept: (Hit & { gone: boolean }) | null;
  /** The expanded card is being edited: whatever the search or `_pinned.md` say, it stays a card. */
  editing: boolean;
}): ListModel {
  const { hits, query, expanded, kept, editing } = input;
  const byPath = new Map(hits.map((hit) => [hit.path, hit]));
  const searching = query.trim() !== "";
  const keptCard = (pinned: boolean): Card | null => {
    if (!kept || !(editing || (kept.gone && !pinned))) return null;
    const { gone, ...hit } = kept;
    return { hit, pinned, level: "expanded", gone };
  };
  const pinnedPaths = new Set<string>();
  const pinned: (Card | Missing)[] = [];
  let held: Card | null = null;
  for (const item of input.pinned) {
    if (item.kind === "url") {
      if (
        !pinnedPaths.has(item.target) &&
        (!searching ||
          `${item.title} ${item.target}`
            .toLocaleLowerCase()
            .includes(query.trim().toLocaleLowerCase()))
      ) {
        pinned.push({
          hit: { path: item.target, title: item.title, tags: [], modified: 0, snippet: null },
          url: item.target,
          pinned: true,
          level: "line",
          gone: false,
        });
      }
      pinnedPaths.add(item.target);
      continue;
    }
    // A hand-edited file may list a note twice; it shows once.
    if (item.kind !== "note" || pinnedPaths.has(item.target)) continue;
    pinnedPaths.add(item.target);
    const hit = byPath.get(item.target);
    if (!hit && item.target === expanded) {
      held = keptCard(true);
      if (held) continue;
    }
    if (item.missing) {
      if (!searching) pinned.push({ target: item.target, title: item.title });
    } else if (hit) {
      pinned.push({
        hit,
        pinned: true,
        level: expanded === hit.path ? "expanded" : item.collapsed ? "line" : "preview",
        gone: false,
      });
    }
  }
  // The edited card the search would hide stays at the top of its section until editing ends.
  if (held) pinned.unshift(held);
  const notes: Card[] = hits
    .filter((hit) => !pinnedPaths.has(hit.path))
    .map((hit) => ({
      hit,
      pinned: false,
      level: expanded === hit.path ? "expanded" : "line",
      gone: false,
    }));
  if (kept && !byPath.has(kept.path) && !pinnedPaths.has(kept.path)) {
    const card = keptCard(false);
    if (card) notes.unshift(card);
  }
  return {
    create: searching && !input.exactTitle ? query.trim() : null,
    pinned,
    notes,
  };
}

export type Row =
  | { key: string; heading: "pinned" | "notes" }
  | { key: string; dateGroup: DateGroup }
  | { key: string; entry: Card | Missing };

/**
 * The list as one sequence, so a card that changes section, by pinning or by an edit of `_pinned.md`,
 * moves instead of being rebuilt. The edited card keeps one key even when its note is renamed: rebuilding
 * it would drop the editor and its unsaved text.
 */
export function rows(
  model: ListModel,
  editing: string,
  headings: ReadonlyMap<string, DateGroup> = new Map(),
): Row[] {
  const key = (entry: Card | Missing) =>
    isMissing(entry)
      ? `missing:${entry.target}`
      : entry.hit.path === editing
        ? "editing"
        : `note:${entry.hit.path}`;
  return [
    ...(model.pinned.length ? [{ key: "heading:pinned", heading: "pinned" as const }] : []),
    ...model.pinned.map((entry) => ({ key: key(entry), entry })),
    ...(model.notes.length ? [{ key: "heading:notes", heading: "notes" as const }] : []),
    ...model.notes.flatMap((entry): Row[] => {
      const dateGroup = headings.get(entry.hit.path);
      return [
        ...(dateGroup ? [{ key: `date:${dateGroup.key}:${entry.hit.path}`, dateGroup }] : []),
        { key: key(entry), entry },
      ];
    }),
  ];
}

/** The selection key of a list entry: a card's path, or a missing item's target. */
export const entryKey = (entry: Card | Missing) =>
  isMissing(entry) ? entry.target : entry.hit.path;

/**
 * Keys of what arrows can select, top to bottom: "create" first when offered, then every pinned entry,
 * missing ones included so the keyboard can remove them, then the notes.
 */
export function navKeys(model: ListModel): string[] {
  return [
    ...(model.create === null ? [] : ["create"]),
    ...[...model.pinned, ...model.notes].map(entryKey),
  ];
}

/** What is selected before any arrow: the first match, else "Create"; nothing for an empty query. */
export function defaultKey(model: ListModel, query: string): string {
  if (!query.trim()) return "";
  return navKeys(model).find((key) => key !== "create") ?? (model.create === null ? "" : "create");
}

/** One step up (-1) or down (1); stays at the ends. With nothing selected, either starts at the top. */
export function move(keys: string[], current: string, delta: -1 | 1): string {
  const at = keys.indexOf(current);
  if (at < 0) return keys[0] ?? "";
  return keys[Math.min(keys.length - 1, Math.max(0, at + delta))];
}

/** A vertical extent on screen, in pixels; `top` above `bottom`. */
export type Span = { top: number; bottom: number };

/**
 * What ↓ (`down`) or ↑ does on a long expanded card: the pixels to scroll the list (`view`) by, or 0
 * to move the selection. While the card goes on past the edge in that direction, the list scrolls
 * by `step`, never past the card's end, so the next press moves on. A card wholly outside the view
 * in that direction is brought to the edge instead of crawled to.
 */
export function arrowScroll(card: Span, view: Span, down: boolean, step: number): number {
  // Fractional layout leaves sub-pixel gaps that must not count as hidden content.
  const slack = 1;
  if (down) {
    if (card.top >= view.bottom) return card.top - view.top;
    const hidden = card.bottom - view.bottom;
    return hidden > slack ? Math.min(step, hidden) : 0;
  }
  if (card.bottom <= view.top) return card.bottom - view.bottom;
  const hidden = view.top - card.top;
  return hidden > slack ? -Math.min(step, hidden) : 0;
}

export type Activation =
  | { action: "create"; text: string }
  | { action: "open"; path: string }
  | { action: "openPage"; url: string }
  | { action: "remove"; target: string };

/** Enter on the selected key: run "Create", expand the card, or remove a missing pinned item. */
export function enter(model: ListModel, key: string): Activation | null {
  if (key === "create")
    return model.create === null ? null : { action: "create", text: model.create };
  if (model.pinned.some((entry) => isMissing(entry) && entry.target === key))
    return { action: "remove", target: key };
  const page = model.pinned.find(
    (entry) => !isMissing(entry) && entry.url && entry.hit.path === key,
  );
  if (page && !isMissing(page) && page.url) return { action: "openPage", url: page.url };
  return navKeys(model).includes(key) ? { action: "open", path: key } : null;
}

/** The level one step further open (→), or null when already expanded. Pinned cards pass through preview. */
export function deeper(card: Card): Level | null {
  if (card.url || card.level === "expanded") return null;
  return card.level === "line" && card.pinned ? "preview" : "expanded";
}

/** The level one step closed (←), or null when already a line. */
export function shallower(card: Card): Level | null {
  if (card.level === "line") return null;
  return card.level === "expanded" && card.pinned ? "preview" : "line";
}

const RESERVED = /^(?:con|prn|aux|nul|com\d|lpt\d)$/i;
const MAX_STEM = 100;
const NEVER = "_pinned";

/**
 * The file a new note gets for the typed text, at the root of the notes folder. It never leaves the
 * folder (no separators, no drive, no "..") and never takes a name in `taken` (compared ignoring
 * case, as Windows does) nor `_pinned.md`, which belongs to the pinned list.
 */
export function noteFileName(text: string, taken: Iterable<string>): string {
  let stem = Array.from(text, (c) => (c < " " || '<>:"/\\|?*'.includes(c) ? " " : c))
    .join("")
    .replace(/\.md$/i, "")
    .replace(/\s+/g, " ")
    .replace(/^[.\s]+|[.\s]+$/g, "");
  stem = Array.from(stem)
    .slice(0, MAX_STEM)
    .join("")
    .trim()
    .replace(/[.\s]+$/, "");
  if (!stem) stem = "Untitled";
  // Device names are reserved on Windows even with an extension.
  if (RESERVED.test(stem.split(".")[0])) stem += "_";
  const used = new Set([`${NEVER}.md`]);
  for (const name of taken) used.add(name.toLowerCase());
  let name = `${stem}.md`;
  for (let n = 2; used.has(name.toLowerCase()); n++) name = `${stem} ${n}.md`;
  return name;
}

/**
 * The path a note gets when renamed to `title`: the same folder and a name free there, as
 * `noteFileName` picks it. `paths` are every note's path; the note's own name counts as free, so a
 * case-only change keeps it.
 */
export function renamedPath(from: string, title: string, paths: Iterable<string>): string {
  const folderOf = (path: string) => path.slice(0, path.lastIndexOf("/") + 1);
  const folder = folderOf(from);
  const taken: string[] = [];
  for (const path of paths) {
    const sibling = folderOf(path).toLowerCase() === folder.toLowerCase();
    if (sibling && path.toLowerCase() !== from.toLowerCase()) taken.push(path.slice(folder.length));
  }
  return folder + noteFileName(title, taken);
}

export function pageHost(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
