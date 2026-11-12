import { describe, expect, test } from "vitest";
import { creationContent, queryCompletion } from "./query";
import {
  FolderIndex,
  folderTags,
  type Listed,
  type Read,
  type SearchIndex,
  search,
  syncIndex,
} from "./search";

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
  test("combines folder matches with title and body without treating the folder as an exact title", async () => {
    const index = await indexOf([
      { path: "Projetos/Ação/plan.md", text: "budget" },
      { path: "plan.md", text: "budget" },
    ]);
    expect(paths(index, "projetos acao plan budget")).toEqual(["Projetos/Ação/plan.md"]);
    expect(search(index, "projetos").exactTitle).toBe(false);
  });

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

test("tag filters require every complete frontmatter tag along with free text", async () => {
  const index = await indexOf([
    { path: "a.md", text: "---\ntags: [Work, revisão]\n---\nbudget" },
    { path: "b.md", text: "---\ntags: [workshop, revisão]\n---\nbudget" },
    { path: "c.md", text: "---\ntags: [work]\n---\nbudget #revisão" },
  ]);
  expect(paths(index, "budget #WORK #revisao")).toEqual(["a.md"]);
  expect(paths(index, "#work #work")).toEqual(["c.md", "a.md"]);
  expect(paths(index, "missing #work")).toEqual([]);
});
test("folder completion excludes root pinned metadata", async () => {
  const notes = [
    { path: "a.md", text: "---\ntags: [Work, work, revisão]\n---" },
    { path: "_pinned.md", text: "---\ntags: [wrong]\n---" },
  ];
  const index = await indexOf(notes);
  expect(folderTags(index, "W")).toEqual(["Work"]);
});

test("folder completion follows note removal", async () => {
  const index = await indexOf([{ path: "a.md", text: "---\ntags: work\n---" }]);
  await syncIndex(index, [], reader([]));
  expect(folderTags(index, "")).toEqual([]);
});

test("date filters combine with all tags and free text", async () => {
  const index = await indexOf([
    {
      path: "a.md",
      // Built in local time: a fixed offset lands in another month in other zones, such as CI's UTC.
      text: `---\ncreated: "${new Date(2026, 9, 31, 23, 59, 59).toISOString()}"\ntags: [work, review]\n---\nbudget`,
    },
    { path: "b.md", text: "---\ndate: '2026-11-01'\ntags: [work, review]\n---\nbudget" },
    { path: "c.md", text: "---\ndate: 2026-10-01\ntags: [work]\n---\nbudget" },
  ]);
  expect(paths(index, "budget #work #review date:2026-10")).toEqual(["a.md"]);
});
test("valid created metadata takes precedence over date and modified time", async () => {
  const index = await indexOf([
    {
      path: "a.md",
      text: "---\ncreated: 2026-10-03\ndate: 2025-01-01\n---",
      modified: +new Date(2024, 0, 1),
    },
  ]);
  expect(paths(index, "date:2026-10")).toEqual(["a.md"]);
});
test("invalid created metadata uses date then modified time", async () => {
  const index = await indexOf([
    { path: "a.md", text: "---\ncreated: invalid\ndate: 2026-10-03\n---" },
    {
      path: "b.md",
      text: "---\ncreated: 2026-02-30\ndate: bad\n---",
      modified: +new Date(2026, 9, 4),
    },
  ]);
  expect(paths(index, "date:2026-10").sort()).toEqual(["a.md", "b.md"]);
});
test("multiple date filters intersect and invalid date tokens are ignored", async () => {
  const index = await indexOf([{ path: "a.md", text: "---\ndate: 2026-10-03\n---" }]);
  expect(paths(index, "date:2026 date:2026-10 date:bad")).toEqual(["a.md"]);
  expect(paths(index, "date:2026-10 date:2026-11")).toEqual([]);
});

test.each(["budget #work", "#work", '#"two words" #review date:today'])(
  "creation from %s remains searchable by its tags",
  async (query) => {
    const content = creationContent(query);
    const index = await indexOf([{ path: "budget.md", text: content }]);
    const tags = query
      .split("date:")[0]
      .replace(/^budget /, "")
      .trim();
    expect(paths(index, tags)).toEqual(["budget.md"]);
  },
);
test("created tag scalars cannot inject extra frontmatter keys", async () => {
  const content = creationContent('#"work\\ncreated: 2000-01-01"');
  const index = await indexOf([{ path: "a.md", text: content, modified: +new Date(2026, 9, 1) }]);
  expect(paths(index, "date:2026-10")).toEqual(["a.md"]);
});
test("only the tag with open completion is ignored while typing", async () => {
  const index = await indexOf([
    { path: "a.md", text: "---\ntags: [work, review]\n---\nbudget" },
    { path: "b.md", text: "---\ntags: work\n---\nbudget" },
  ]);
  const query = "budget #review #wor";
  expect(
    search(index, query, new Date(), queryCompletion(query, query.length)).hits.map(
      (hit) => hit.path,
    ),
  ).toEqual(["a.md"]);
  expect(paths(index, query)).toEqual([]);
});
test("folder completion folds accents and case", async () => {
  const index = await indexOf([{ path: "a.md", text: "---\ntags: revisão\n---" }]);
  expect(folderTags(index, "REVI")).toEqual(["revisão"]);
});
test("folder completion is capped at fifty distinct suggestions", async () => {
  const index = await indexOf([
    {
      path: "a.md",
      text: `---\ntags: [${Array.from({ length: 100 }, (_, i) => `tag${i}`).join(",")}]\n---`,
    },
  ]);
  expect(folderTags(index, "tag")).toHaveLength(50);
});
