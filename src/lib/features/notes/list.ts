// The note list as a pure model: which cards and rows exist, in which order, and what each keyboard
// action does. The component only draws it and runs the effects. No IPC here.
import type { PinnedItem } from "../../ipc";
import type { Hit } from "./search";

/** `line` is a one-line row for a note that is not pinned. */
export type Level = "line" | "preview" | "expanded";
export type Card = {
  hit: Hit;
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
  /** The expanded note when it is no longer listed: shown with its last content. */
  kept: Hit | null;
}): ListModel {
  const { hits, query, expanded } = input;
  const byPath = new Map(hits.map((hit) => [hit.path, hit]));
  const searching = query.trim() !== "";
  const pinnedPaths = new Set<string>();
  const pinned: (Card | Missing)[] = [];
  for (const item of input.pinned) {
    // Web pages come with M4.
    // A hand-edited file may list a note twice; it shows once.
    if (item.kind !== "note" || pinnedPaths.has(item.target)) continue;
    pinnedPaths.add(item.target);
    const hit = byPath.get(item.target);
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
  const notes: Card[] = hits
    .filter((hit) => !pinnedPaths.has(hit.path))
    .map((hit) => ({
      hit,
      pinned: false,
      level: expanded === hit.path ? "expanded" : "line",
      gone: false,
    }));
  const kept = input.kept;
  if (kept && !byPath.has(kept.path) && !pinnedPaths.has(kept.path))
    notes.unshift({ hit: kept, pinned: false, level: "expanded", gone: true });
  return {
    create: searching && !input.exactTitle ? query.trim() : null,
    pinned,
    notes,
  };
}

/** Keys of what arrows can select, top to bottom: the card paths, and "create" first when offered. */
export function navKeys(model: ListModel): string[] {
  const cards = [...model.pinned.filter((e): e is Card => !isMissing(e)), ...model.notes];
  return [...(model.create === null ? [] : ["create"]), ...cards.map((card) => card.hit.path)];
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

export type Activation = { action: "create"; text: string } | { action: "open"; path: string };

/** Enter on the selected key: run "Create", or expand the card. */
export function enter(model: ListModel, key: string): Activation | null {
  if (key === "create")
    return model.create === null ? null : { action: "create", text: model.create };
  return navKeys(model).includes(key) ? { action: "open", path: key } : null;
}

/** The level one step further open (→), or null when already expanded. Pinned cards pass through preview. */
export function deeper(card: Card): Level | null {
  if (card.level === "expanded") return null;
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
