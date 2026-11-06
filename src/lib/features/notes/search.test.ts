import { describe, expect, test } from "vitest";
import { FolderIndex, type Listed, type Read, type SearchIndex, search, syncIndex } from "./search";

type Note = { path: string; text: string; modified?: number };

async function indexOf(notes: Note[]): Promise<SearchIndex> {
  const index: SearchIndex = new Map();
  await syncIndex(index, listing(notes), reader(notes));
  return index;
}
const listing = (notes: Note[]): Listed[] =>
  notes.map((n, i) => ({
    path: n.path,
    name: n.path.replace(/^.*\//, "").replace(/\.md$/, ""),
    modified: n.modified ?? i + 1,
  }));
const reader = (notes: Note[]) => async (paths: string[]) =>
  notes
    .map((n, i): Read => ({ path: n.path, text: n.text, modified: n.modified ?? i + 1 }))
    .filter((n) => paths.includes(n.path));
const snippetOf = (index: SearchIndex, query: string) => {
  const snippet = search(index, query).hits[0]?.snippet;
  if (!snippet) throw new Error("no snippet");
  return snippet;
};
const paths = (index: SearchIndex, query: string) =>
  search(index, query).hits.map((hit) => hit.path);

describe("matching", () => {
  test("hides the root pinned file from the list, matches and exact-title detection", async () => {
    const index = await indexOf([
      { path: "_PINNED.MD", text: "private list" },
      { path: "note.md", text: "private note" },
      { path: "folder/_pinned.md", text: "ordinary nested note" },
    ]);
    expect(paths(index, "")).toEqual(["folder/_pinned.md", "note.md"]);
    expect(paths(index, "private")).toEqual(["note.md"]);
    expect(search(index, "_PINNED.MD")).toEqual({ hits: [], exactTitle: false });
  });

  test("ignores case and accents in both the query and the note", async () => {
    const index = await indexOf([{ path: "a.md", text: "Reunião de AÇÃO\n" }]);
    expect(paths(index, "reuniao acao")).toEqual(["a.md"]);
    expect(paths(index, "REUNIÃO")).toEqual(["a.md"]);
  });

  test("matches accents written as a base letter plus a combining mark", async () => {
    const index = await indexOf([{ path: "a.md", text: "cafe\u0301 com leite" }]);
    expect(paths(index, "café")).toEqual(["a.md"]);
  });

  test("requires every word, each anywhere in title, tags or body", async () => {
    const index = await indexOf([
      { path: "plan.md", text: "---\ntags: [work]\n---\nbudget for next year" },
      { path: "other.md", text: "budget only" },
    ]);
    expect(paths(index, "plan work budget")).toEqual(["plan.md"]);
    expect(paths(index, "budget")).toHaveLength(2);
    expect(paths(index, "budget missing")).toEqual([]);
  });

  test("uses the frontmatter title, else the file name, and never searches other frontmatter", async () => {
    const index = await indexOf([
      { path: "a.md", text: "---\ntitle: Garden\nurl: secret\n---\nbody" },
      { path: "dir/Shed.md", text: "body" },
    ]);
    expect(search(index, "garden").hits[0].title).toBe("Garden");
    expect(search(index, "shed").hits[0].title).toBe("Shed");
    expect(paths(index, "secret")).toEqual([]);
  });

  test("an empty query lists every note, newest first, without snippets", async () => {
    const index = await indexOf([
      { path: "old.md", text: "x", modified: 1 },
      { path: "new.md", text: "y", modified: 9 },
    ]);
    const { hits, exactTitle } = search(index, "  ");
    expect(hits.map((h) => h.path)).toEqual(["new.md", "old.md"]);
    expect(hits.every((h) => h.snippet === null)).toBe(true);
    expect(exactTitle).toBe(false);
  });
});

describe("ranking", () => {
  test("orders exact title, title prefix, title contains, tag, body", async () => {
    const index = await indexOf([
      { path: "body.md", text: "mentions apple here", modified: 5 },
      { path: "tag.md", text: "---\ntags: [apple]\n---\nnothing", modified: 4 },
      { path: "contains.md", text: "---\ntitle: Green apple pie\n---\n", modified: 3 },
      { path: "prefix.md", text: "---\ntitle: Apple pie\n---\n", modified: 2 },
      { path: "exact.md", text: "---\ntitle: apple\n---\n", modified: 1 },
    ]);
    expect(paths(index, "apple")).toEqual([
      "exact.md",
      "prefix.md",
      "contains.md",
      "tag.md",
      "body.md",
    ]);
  });

  test("ties go to the most recently modified note", async () => {
    const index = await indexOf([
      { path: "a.md", text: "word", modified: 1 },
      { path: "b.md", text: "word", modified: 3 },
      { path: "c.md", text: "word", modified: 2 },
    ]);
    expect(paths(index, "word")).toEqual(["b.md", "c.md", "a.md"]);
  });

  test("a multi-word query is a prefix only when the title starts with all of it", async () => {
    const index = await indexOf([
      { path: "x.md", text: "---\ntitle: Pie apple\n---\napple pie", modified: 2 },
      { path: "y.md", text: "---\ntitle: Apple pie recipe\n---\n", modified: 1 },
    ]);
    expect(paths(index, "apple pie")).toEqual(["y.md", "x.md"]);
  });
});

describe("exact title", () => {
  test("is true only when a title equals the whole query, ignoring case, accents and spacing", async () => {
    const index = await indexOf([{ path: "Ação diária.md", text: "" }]);
    expect(search(index, "acao  DIARIA").exactTitle).toBe(true);
    expect(search(index, "acao").exactTitle).toBe(false);
    expect(search(index, "acao diaria extra").exactTitle).toBe(false);
  });
});

describe("snippet", () => {
  const body = `${"filler ".repeat(30)}The Ação plan is here. ${"tail ".repeat(40)}`;

  test("surrounds the first body match, with ranges relative to the snippet", async () => {
    const index = await indexOf([{ path: "a.md", text: body }]);
    const { text, ranges, before, after } = snippetOf(index, "acao");
    expect(ranges).toHaveLength(1);
    expect(text.slice(ranges[0].from, ranges[0].to)).toBe("Ação");
    expect(before && after).toBe(true);
    expect(text.length).toBeLessThan(body.length / 2);
  });

  test("marks every query word in the window, merging overlaps", async () => {
    const index = await indexOf([{ path: "a.md", text: "a Quick brown fox, quick" }]);
    const { text, ranges } = snippetOf(index, "quick brown qui");
    expect(ranges.map((r) => text.slice(r.from, r.to))).toEqual(["Quick", "brown", "quick"]);
  });

  test("keeps offsets right when accents were decomposed in the source", async () => {
    const index = await indexOf([{ path: "a.md", text: "ca\u0301fe\u0301 e pao" }]);
    const { text, ranges } = snippetOf(index, "pao");
    expect(text.slice(ranges[0].from, ranges[0].to)).toBe("pao");
  });

  test("is plain text on one line, never markup, and absent when only title or tags match", async () => {
    const index = await indexOf([
      { path: "html.md", text: "<b>x</b>\nneedle\r\nnext", modified: 2 },
      { path: "needle.md", text: "nothing", modified: 1 },
    ]);
    const [titled, html] = search(index, "needle").hits;
    expect(html.snippet?.text).toBe("<b>x</b> needle next");
    expect(titled.snippet).toBeNull();
  });

  test("ignores the frontmatter", async () => {
    const index = await indexOf([{ path: "a.md", text: "---\ntitle: T\n---\nbody here" }]);
    expect(search(index, "title").hits).toEqual([]);
    expect(snippetOf(index, "body").text).toBe("body here");
  });
});

describe("syncIndex", () => {
  test("rereads only notes that are new or modified, and forgets removed ones", async () => {
    const notes: Note[] = [
      { path: "a.md", text: "alpha", modified: 1 },
      { path: "b.md", text: "beta", modified: 1 },
    ];
    const index: SearchIndex = new Map();
    const asked: string[][] = [];
    const read = async (p: string[]) => {
      asked.push(p);
      return reader(notes)(p);
    };
    await syncIndex(index, listing(notes), read);
    notes[1] = { path: "b.md", text: "gamma", modified: 2 };
    notes.push({ path: "c.md", text: "delta", modified: 1 });
    await syncIndex(index, listing(notes), read);
    expect(asked).toEqual([
      ["a.md", "b.md"],
      ["b.md", "c.md"],
    ]);
    expect(paths(index, "beta")).toEqual([]);
    expect(paths(index, "gamma")).toEqual(["b.md"]);
    notes.shift();
    await syncIndex(index, listing(notes), read);
    expect(paths(index, "alpha")).toEqual([]);
    expect(asked).toHaveLength(2);
  });

  test("an unreadable note stays findable by name and is retried on the next sync", async () => {
    const index: SearchIndex = new Map();
    const listed: Listed[] = [{ path: "broken.md", name: "broken", modified: 1 }];
    await syncIndex(index, listed, async () => []);
    expect(paths(index, "broken")).toEqual(["broken.md"]);
    let retried = false;
    await syncIndex(index, listed, async () => {
      retried = true;
      return [{ path: "broken.md", text: "now readable", modified: 1 }];
    });
    expect(retried).toBe(true);
    expect(paths(index, "readable")).toEqual(["broken.md"]);
  });
});

describe("FolderIndex", () => {
  test("a sync of the previous folder that ends after a reset is neither shown nor reused", async () => {
    let release = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const listed: Listed[] = [{ path: "a.md", name: "a", modified: 1 }];
    // The same path and modified time in both folders: only the text tells them apart.
    let calls = 0;
    const folder = new FolderIndex(async (p) => {
      const first = calls++ === 0;
      if (first) await gate;
      return p.map((path) => ({ path, text: first ? "alpha" : "beta", modified: 1 }));
    });
    const previous = folder.sync(listed);
    folder.reset();
    const current = folder.sync(listed);
    release();
    await Promise.all([previous, current]);
    expect(paths(folder.index, "alpha")).toEqual([]);
    expect(paths(folder.index, "beta")).toEqual(["a.md"]);
  });
});
