// In-memory note search: pure functions, no IPC. The caller supplies the listing and a reader for
// note texts, and renders the results; snippets are plain text plus offsets, never HTML.
import { frontmatter } from "./markdown";

/** What the listing says about a note. */
export type Listed = { path: string; name: string; modified: number };
/** A note's raw text as read from disk. */
export type Read = { path: string; text: string; modified: number };

type Entry = {
  path: string;
  title: string;
  tags: string[];
  modified: number;
  /** The text after the frontmatter. */
  body: string;
  /** Folded forms, for matching. */
  titleKey: string;
  tagKeys: string[];
  bodyKey: string;
  /** Every character keeps its length when folded, so offsets in `bodyKey` are offsets in `body`. */
  aligned: boolean;
  /** The text could not be read: only the name is searchable until the next sync. */
  unread: boolean;
};

export type SearchIndex = Map<string, Entry>;

const BATCH = 200;
const BEFORE = 40;
const AFTER = 80;

const folded = new Map<string, string>();
/** One character, lowercased and without accents. Per character so offsets can be mapped back. */
function foldChar(char: string): string {
  let result = folded.get(char);
  if (result === undefined) {
    result = char.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
    folded.set(char, result);
  }
  return result;
}

/** Case- and accent-insensitive form of `text`. */
export function fold(text: string): string {
  return text.replace(/[A-Z]|\P{ASCII}/gu, foldChar);
}

/** `fold(text)` plus, for each folded unit, the offset of the character it came from. */
function foldWithOrigin(text: string): { key: string; origin: number[] } {
  let key = "",
    at = 0;
  const origin: number[] = [];
  for (const char of text) {
    const part = /[A-Z]|\P{ASCII}/u.test(char) ? foldChar(char) : char;
    for (let i = 0; i < part.length; i++) origin.push(at);
    key += part;
    at += char.length;
  }
  origin.push(at);
  return { key, origin };
}

function words(query: string): string[] {
  return fold(query).split(/\s+/).filter(Boolean);
}

function build(listed: Listed, read: Read | null): Entry {
  let title = listed.name,
    tags: string[] = [],
    body = "";
  if (read) {
    const lines = read.text.replace(/\r\n?/g, "\n").split("\n");
    const meta = frontmatter((n) => lines[n - 1], lines.length);
    body = (meta ? lines.slice(meta.end) : lines).join("\n");
    if (meta) {
      title = meta.title ?? title;
      tags = meta.tags;
    }
  }
  return {
    path: listed.path,
    title,
    tags,
    modified: read?.modified ?? listed.modified,
    body,
    titleKey: collapse(title),
    tagKeys: tags.map(fold),
    bodyKey: fold(body),
    aligned: [...body.matchAll(/\P{ASCII}/gu)].every(([c]) => foldChar(c).length === c.length),
    unread: !read,
  };
}

function collapse(text: string): string {
  return fold(text).replace(/\s+/g, " ").trim();
}

/**
 * Brings `index` in line with the listing: forgets removed notes and rereads, through `read`, only the
 * notes that are new or whose modified time changed. Run one sync at a time: the index changes once the
 * reading is done.
 */
export async function syncIndex(
  index: SearchIndex,
  listed: Listed[],
  read: (paths: string[]) => Promise<Read[]>,
): Promise<void> {
  const stale = listed.filter((note) => {
    const entry = index.get(note.path);
    return !entry || entry.unread || entry.modified !== note.modified;
  });
  const texts = new Map<string, Read>();
  for (let i = 0; i < stale.length; i += BATCH)
    for (const item of await read(stale.slice(i, i + BATCH).map((note) => note.path)))
      texts.set(item.path, item);
  const present = new Set(listed.map((note) => note.path));
  for (const path of index.keys()) if (!present.has(path)) index.delete(path);
  for (const note of stale) index.set(note.path, build(note, texts.get(note.path) ?? null));
}

/**
 * The index of the current folder, with its syncs run one at a time in call order. `reset` starts an
 * empty index for another folder: a sync called before it fills only the old index, so a slow read of
 * the previous folder never shows up in, or is reused by, the new one.
 */
export class FolderIndex {
  index: SearchIndex = new Map();
  #queue: Promise<void> = Promise.resolve();
  readonly #read: (paths: string[]) => Promise<Read[]>;

  constructor(read: (paths: string[]) => Promise<Read[]>) {
    this.#read = read;
  }

  reset() {
    this.index = new Map();
  }

  sync(listed: Listed[]): Promise<void> {
    const target = this.index;
    const run = this.#queue.then(() => syncIndex(target, listed, this.#read));
    this.#queue = run.catch(() => {});
    return run;
  }
}

/** The root pinned list is metadata, not a note shown in the list or search. */
export const isPinnedList = (path: string) => path.toLowerCase() === "_pinned.md";

export type Snippet = {
  text: string;
  /** Offsets into `text` of every match, in order. */
  ranges: { from: number; to: number }[];
  /** The note continues before / after the snippet. */
  before: boolean;
  after: boolean;
};
export type Hit = {
  path: string;
  title: string;
  tags: string[];
  modified: number;
  /** Around the first match in the body; null when the match is in the title or tags only. */
  snippet: Snippet | null;
};
export type Results = {
  hits: Hit[];
  /** Some note's title equals the query, so the UI should not offer to create it. */
  exactTitle: boolean;
};

// Lower is better.
const EXACT = 0,
  PREFIX = 1,
  TITLE = 2,
  TAG = 3,
  BODY = 4;

function snippet(entry: Entry, terms: string[]): Snippet | null {
  let first = -1,
    length = 0;
  for (const term of terms) {
    const at = entry.bodyKey.indexOf(term);
    if (at >= 0 && (first < 0 || at < first)) {
      first = at;
      length = term.length;
    }
  }
  if (first < 0) return null;
  const { key, origin } = entry.aligned
    ? { key: entry.bodyKey, origin: null }
    : foldWithOrigin(entry.body);
  const original = (at: number) => (origin ? origin[at] : at);
  const from = Math.max(0, first - BEFORE),
    to = Math.min(key.length, first + length + AFTER);
  const start = original(from),
    end = original(to);
  const window = key.slice(from, to);
  const ranges: { from: number; to: number }[] = [];
  for (const term of terms)
    for (let at = window.indexOf(term); at >= 0; at = window.indexOf(term, at + term.length))
      ranges.push({
        from: original(from + at) - start,
        to: original(from + at + term.length) - start,
      });
  ranges.sort((a, b) => a.from - b.from || b.to - a.to);
  // Overlapping terms ("ab" and "bc" in "abc") become one range.
  const merged: typeof ranges = [];
  for (const range of ranges) {
    const last = merged[merged.length - 1];
    if (last && range.from <= last.to) last.to = Math.max(last.to, range.to);
    else merged.push(range);
  }
  return {
    // One-for-one so the offsets stay valid.
    text: entry.body.slice(start, end).replace(/\s/g, " "),
    ranges: merged,
    before: start > 0,
    after: end < entry.body.length,
  };
}

/**
 * Every word of `query` must appear in the title (frontmatter title, else the file name), the tags or
 * the body, ignoring case and accents. Order: exact title, title starting with the query, a word in
 * the title, a word in a tag, body only; then newest first. An empty query lists every note by
 * modified time.
 */
export function search(index: SearchIndex, query: string): Results {
  const terms = words(query);
  const whole = terms.join(" ");
  const ranked: { entry: Entry; tier: number }[] = [];
  let exactTitle = false;
  for (const entry of index.values()) {
    if (isPinnedList(entry.path)) continue;
    if (terms.length && entry.titleKey === whole) exactTitle = true;
    const inTitle = terms.map((term) => entry.titleKey.includes(term));
    const inTags = terms.map((term) => entry.tagKeys.some((tag) => tag.includes(term)));
    if (!terms.every((term, i) => inTitle[i] || inTags[i] || entry.bodyKey.includes(term)))
      continue;
    const tier = !terms.length
      ? BODY
      : entry.titleKey === whole
        ? EXACT
        : entry.titleKey.startsWith(whole)
          ? PREFIX
          : inTitle.some(Boolean)
            ? TITLE
            : inTags.some(Boolean)
              ? TAG
              : BODY;
    ranked.push({ entry, tier });
  }
  ranked.sort(
    (a, b) =>
      a.tier - b.tier ||
      b.entry.modified - a.entry.modified ||
      (a.entry.path < b.entry.path ? -1 : 1),
  );
  return {
    exactTitle,
    hits: ranked.map(({ entry }) => ({
      path: entry.path,
      title: entry.title,
      tags: entry.tags,
      modified: entry.modified,
      snippet: terms.length ? snippet(entry, terms) : null,
    })),
  };
}
