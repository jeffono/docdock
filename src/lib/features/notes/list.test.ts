import { describe, expect, it } from "vitest";
import type { PinnedItem } from "../../ipc";
import {
  arrowScroll,
  buildList,
  type Card,
  deeper,
  defaultKey,
  enter,
  escapeAction,
  fileLabel,
  folderLabels,
  isMissing,
  isUntitled,
  jumpSection,
  move,
  movedPath,
  navKeys,
  noteFileName,
  renamedPath,
  revealScroll,
  rows,
  shallower,
} from "./list";
import type { Hit } from "./search";

const hit = (path: string): Hit => ({ path, title: path, tags: [], modified: 1, snippet: null });
const item = (target: string, extra: Partial<PinnedItem> = {}): PinnedItem => ({
  target,
  title: target,
  kind: "note",
  missing: false,
  collapsed: false,
  ...extra,
});
const base = {
  hits: ["a.md", "b.md", "c.md"].map(hit),
  exactTitle: false,
  query: "",
  pinned: [] as PinnedItem[],
  expanded: "",
  kept: null as (Hit & { gone: boolean }) | null,
  editing: false,
};
const paths = (entries: (Card | { target: string })[]) =>
  entries.map((e) => ("target" in e ? e.target : e.hit.path));

describe("Escape layers", () => {
  it.each([false, true])("leaves editing first even when the bar is focused: %s", (inBar) => {
    expect(escapeAction(true, inBar)).toBe("leaveEditor");
  });

  it("returns from reading to the search bar", () => {
    expect(escapeAction(false, false)).toBe("focusSearch");
  });

  it("closes from the bar without a search-clearing layer", () => {
    expect(escapeAction(false, true)).toBe("close");
  });
});

describe("sections", () => {
  it("keeps pinned cards in file order and takes them out of Notes", () => {
    const model = buildList({ ...base, pinned: [item("c.md"), item("a.md", { collapsed: true })] });
    expect(paths(model.pinned)).toEqual(["c.md", "a.md"]);
    expect(paths(model.notes)).toEqual(["b.md"]);
    expect(model.pinned.map((e) => !isMissing(e) && e.level)).toEqual(["preview", "line"]);
  });

  it("shows a note pinned twice once", () => {
    expect(paths(buildList({ ...base, pinned: [item("a.md"), item("a.md")] }).pinned)).toEqual([
      "a.md",
    ]);
  });

  it("lists a missing pinned note only while not searching", () => {
    const pinned = [item("gone.md", { missing: true })];
    expect(paths(buildList({ ...base, pinned }).pinned)).toEqual(["gone.md"]);
    expect(buildList({ ...base, pinned, query: "x" }).pinned).toEqual([]);
  });

  it("hides pinned notes the search filtered out but keeps matches as cards", () => {
    const model = buildList({
      ...base,
      hits: [hit("b.md")],
      query: "b",
      pinned: [item("a.md"), item("b.md")],
    });
    expect(paths(model.pinned)).toEqual(["b.md"]);
    expect(model.notes).toEqual([]);
  });

  it("expands one card and keeps a vanished expanded note on screen", () => {
    const model = buildList({ ...base, expanded: "a.md", pinned: [item("b.md")] });
    expect(model.notes[0].level).toBe("expanded");
    const gone = { ...hit("x.md"), gone: true };
    const kept = buildList({ ...base, hits: [], expanded: "x.md", kept: gone });
    expect(kept.notes).toMatchObject([{ gone: true, level: "expanded" }]);
  });
});

describe("edited card", () => {
  const filtered = { ...hit("a.md"), gone: false };
  const searching = { ...base, hits: [hit("b.md")], query: "b", expanded: "a.md", kept: filtered };

  it("stays at the top while the search hides it, and only while editing", () => {
    expect(paths(buildList({ ...searching, editing: true }).notes)).toEqual(["a.md", "b.md"]);
    expect(paths(buildList(searching).notes)).toEqual(["b.md"]);
  });

  it("stays first among pinned cards while the search hides it", () => {
    const pinned = [item("b.md"), item("a.md")];
    const model = buildList({ ...searching, pinned, editing: true });
    expect(paths(model.pinned)).toEqual(["a.md", "b.md"]);
    expect(model.pinned[0]).toMatchObject({ pinned: true, level: "expanded" });
  });

  it("stays a card when its pinned note is deleted, instead of a missing row", () => {
    const input = {
      ...base,
      hits: [],
      pinned: [item("a.md", { missing: true })],
      expanded: "a.md",
      kept: { ...hit("a.md"), gone: true },
    };
    expect(buildList({ ...input, editing: true }).pinned).toMatchObject([{ gone: true }]);
    expect(isMissing(buildList(input).pinned[0])).toBe(true);
  });

  it("keeps one key across sections and renames, so the editor is not rebuilt", () => {
    const unpinned = buildList({ ...base, expanded: "a.md", editing: true });
    const pinned = buildList({ ...base, expanded: "a.md", editing: true, pinned: [item("a.md")] });
    const key = (model: typeof pinned, path: string) =>
      rows(model, path).find(
        (row) => "entry" in row && !isMissing(row.entry) && row.entry.hit.path === path,
      )?.key;
    expect(key(unpinned, "a.md")).toBe(key(pinned, "a.md"));
    const renamed = buildList({ ...base, hits: [hit("z.md")], expanded: "z.md", editing: true });
    expect(key(renamed, "z.md")).toBe(key(unpinned, "a.md"));
  });
});

describe("create item", () => {
  it("is offered for the trimmed query unless a title equals it", () => {
    expect(buildList({ ...base, query: "  new idea " }).create).toBe("new idea");
    expect(buildList({ ...base, query: "a", exactTitle: true }).create).toBeNull();
    expect(buildList({ ...base, query: "   " }).create).toBeNull();
  });
});

describe("keyboard", () => {
  const model = buildList({ ...base, query: "q", pinned: [item("c.md")] });

  it("walks Create, pinned cards, then notes", () => {
    expect(navKeys(model)).toEqual(["create", "c.md", "a.md", "b.md"]);
  });

  it("selects the first match, else Create, else nothing", () => {
    expect(defaultKey(model, "q")).toBe("c.md");
    expect(defaultKey(buildList({ ...base, hits: [], query: "q" }), "q")).toBe("create");
    expect(defaultKey(buildList({ ...base, hits: [], query: "q", exactTitle: true }), "q")).toBe(
      "",
    );
    expect(defaultKey(buildList(base), "")).toBe("");
  });

  it("moves without wrapping and starts at the top", () => {
    const keys = navKeys(model);
    expect(move(keys, "", 1)).toBe("create");
    expect(move(keys, "create", -1)).toBe("create");
    expect(move(keys, "b.md", 1)).toBe("b.md");
    expect(move(keys, "c.md", 1)).toBe("a.md");
  });

  it("makes Enter create on Create and open on a card", () => {
    expect(enter(model, "create")).toEqual({ action: "create", text: "q" });
    expect(enter(model, "a.md")).toEqual({ action: "open", path: "a.md" });
    expect(enter(model, "nope.md")).toBeNull();
    expect(enter(buildList(base), "create")).toBeNull();
  });

  it("selects a missing pinned item in its place and makes Enter remove it", () => {
    const withMissing = buildList({
      ...base,
      pinned: [item("c.md"), item("gone.md", { missing: true })],
    });
    expect(navKeys(withMissing)).toEqual(["c.md", "gone.md", "a.md", "b.md"]);
    expect(enter(withMissing, "gone.md")).toEqual({ action: "remove", target: "gone.md" });
  });

  it("opens and closes one level, through preview only for pinned cards", () => {
    const card = (pinned: boolean, level: Card["level"]): Card => ({
      hit: hit("a.md"),
      pinned,
      level,
      gone: false,
    });
    expect(deeper(card(true, "line"))).toBe("preview");
    expect(deeper(card(true, "preview"))).toBe("expanded");
    expect(deeper(card(false, "line"))).toBe("expanded");
    expect(deeper(card(false, "expanded"))).toBeNull();
    expect(shallower(card(true, "expanded"))).toBe("preview");
    expect(shallower(card(true, "preview"))).toBe("line");
    expect(shallower(card(false, "expanded"))).toBe("line");
    expect(shallower(card(false, "line"))).toBeNull();
  });
});

describe("arrowScroll", () => {
  const view = { top: 100, bottom: 500 };

  // Bug: ↓ jumps to the next item while the expanded card still has unread text below the view.
  it("scrolls by a step while the card goes on below, then lets ↓ move", () => {
    expect(arrowScroll({ top: 120, bottom: 2000 }, view, true, 60)).toBe(60);
    expect(arrowScroll({ top: 120, bottom: 530 }, view, true, 60)).toBe(30);
    expect(arrowScroll({ top: 120, bottom: 500.5 }, view, true, 60)).toBe(0);
  });

  it("mirrors with ↑ on the card top", () => {
    expect(arrowScroll({ top: -900, bottom: 400 }, view, false, 60)).toBe(-60);
    expect(arrowScroll({ top: 80, bottom: 400 }, view, false, 60)).toBe(-20);
    expect(arrowScroll({ top: 99.5, bottom: 400 }, view, false, 60)).toBe(0);
  });

  // Bug: after PgUp leaves the selected card far below, ↓ crawls toward it three lines at a time.
  it("brings a card wholly past the edge to it, and moves when it is behind", () => {
    expect(arrowScroll({ top: 900, bottom: 3000 }, view, true, 60)).toBe(800);
    expect(arrowScroll({ top: -3000, bottom: -200 }, view, false, 60)).toBe(-700);
    expect(arrowScroll({ top: -3000, bottom: -200 }, view, true, 60)).toBe(0);
    expect(arrowScroll({ top: 900, bottom: 3000 }, view, false, 60)).toBe(0);
  });
});

// Bug: a note the user named would be renamed when left, or an untitled one would keep its name.
it("only names given to notes created without text count as untitled", () => {
  for (const path of ["Untitled.md", "Untitled 2.md", "a/Untitled 12.md"])
    expect(isUntitled(path)).toBe(true);
  for (const path of ["untitled.md", "Untitled draft.md", "Untitled 2.txt", "Untitled/Ideas.md"])
    expect(isUntitled(path)).toBe(false);
});

// Bug: two notes with one frontmatter title look the same, or every title with a colon shows its file.
it("shows a note's file name only where it differs from the title beyond what files cannot hold", () => {
  expect(fileLabel("Projeto 2.md", "Projeto")).toBe("Projeto 2.md");
  expect(fileLabel("a/projeto.md", "Projeto")).toBe("projeto.md");
  expect(fileLabel("a/Plano Q3.md", "Plano: Q3")).toBeNull();
  expect(fileLabel("CON_.md", "CON")).toBeNull();
});

describe("noteFileName", () => {
  it("stays at the folder root whatever is typed", () => {
    for (const text of ["../../evil", "a/b\\c", "C:\\Windows\\x", "..", "...", "\u0000x<>|?*"]) {
      const name = noteFileName(text, []);
      expect(name).toMatch(/^[^/\\:*?"<>|]+\.md$/);
      expect([...name].every((c) => c >= " ")).toBe(true);
      expect(name.startsWith(".")).toBe(false);
    }
    expect(noteFileName("../../evil", [])).toBe("evil.md");
    expect(noteFileName("..", [])).toBe("Untitled.md");
  });

  it("avoids Windows device names, trailing dots and a doubled extension", () => {
    expect(noteFileName("CON", [])).toBe("CON_.md");
    expect(noteFileName("nul.txt", [])).toBe("nul.txt_.md");
    expect(noteFileName("Ideas.", [])).toBe("Ideas.md");
    expect(noteFileName("Ideas.MD", [])).toBe("Ideas.md");
  });

  it("never clobbers: numbers a taken name, ignoring case, and keeps _pinned.md free", () => {
    expect(noteFileName("Ideas", ["ideas.md"])).toBe("Ideas 2.md");
    expect(noteFileName("Ideas", ["IDEAS.md", "Ideas 2.md"])).toBe("Ideas 3.md");
    expect(noteFileName("_pinned", [])).toBe("_pinned 2.md");
  });

  it("caps very long names", () => {
    expect(noteFileName("x".repeat(500), []).length).toBeLessThanOrEqual(103);
  });
});

it("keeps web pages between notes in file order without loading content", () => {
  const model = buildList({
    ...base,
    pinned: [
      item("b.md"),
      item("https://example.com/a%28b%29", { kind: "url", title: "Page", collapsed: false }),
      item("a.md"),
    ],
  });
  expect(paths(model.pinned)).toEqual(["b.md", "https://example.com/a%28b%29", "a.md"]);
  expect(model.pinned[1]).toMatchObject({ level: "line", url: "https://example.com/a%28b%29" });
});
it("activates a pinned page as a page rather than a note read", () => {
  const url = "https://example.com/";
  expect(enter(buildList({ ...base, pinned: [item(url, { kind: "url" })] }), url)).toEqual({
    action: "openPage",
    url,
  });
});

describe("renamedPath", () => {
  const paths = ["sub/Note.md", "sub/taken.md", "taken 2.md", "other/x.md"];
  it("stays in the note's folder and skips names taken there only", () => {
    expect(renamedPath("sub/Note.md", "Taken", paths)).toBe("sub/Taken 2.md");
    expect(renamedPath("sub/Note.md", "x", paths)).toBe("sub/x.md");
  });
  it("lets a case-only change keep the note's own name", () => {
    expect(renamedPath("sub/Note.md", "note", paths)).toBe("sub/note.md");
  });
});

describe("revealScroll", () => {
  const view = { top: 0, bottom: 500 };
  it("brings an opened card's end into view without pushing its top out", () => {
    expect(revealScroll({ top: 400, bottom: 700 }, view)).toBe(200);
    expect(revealScroll({ top: 300, bottom: 1200 }, view)).toBe(300);
  });
  it("leaves a shown card and brings one above the view to the top", () => {
    expect(revealScroll({ top: 100, bottom: 400 }, view)).toBe(0);
    expect(revealScroll({ top: -50, bottom: 300 }, view)).toBe(-50);
  });
});

describe("movedPath", () => {
  const paths = ["Note.md", "sub/note.md", "sub/Note 2.md", "sub/deep/x.md"];
  it("keeps the name and numbers it when taken in the target folder only", () => {
    expect(movedPath("Note.md", "sub", paths)).toBe("sub/Note 3.md");
    expect(movedPath("sub/deep/x.md", "sub", paths)).toBe("sub/x.md");
    expect(movedPath("sub/note.md", "", paths)).toBe("note 2.md");
  });
  it("never takes the pinned list at the root", () => {
    expect(movedPath("sub/_pinned.md", "", [])).toBe("_pinned 2.md");
  });
});

// Without these labels an empty folder, or one with only pinned notes, cannot be deleted.
it("folder rows keep empty folders and folders whose notes are all pinned accessible", () => {
  const model = buildList({
    ...base,
    hits: [hit("parent/pinned.md")],
    pinned: [item("parent/pinned.md")],
  });
  const result = rows(model, "", new Map(), ["parent", "parent/empty"]);
  expect(
    result.filter((row) => "group" in row).map((row) => "group" in row && row.group.key),
  ).toEqual(["folder:parent", "folder:parent/empty"]);
});

it("folder labels exclude attachments but keep note ancestors and empty folders", () => {
  expect(
    folderLabels(
      [{ path: "parent/sub/a.md" }],
      [
        { path: "parent", empty: false },
        { path: "parent/sub", empty: false },
        { path: "attachments", empty: false },
        { path: "empty", empty: true },
        { path: "only-empty-children", empty: false },
      ],
    ),
  ).toEqual(["parent", "parent/sub", "empty"]);
});
it("arrows include folder menus in displayed order", () => {
  const model = buildList({ ...base, hits: [hit("sub/a.md"), hit("root.md")] });
  expect(navKeys(model, rows(model, "", new Map(), ["empty", "sub"]))).toEqual([
    "root.md",
    "folder:empty",
    "folder:sub",
    "sub/a.md",
  ]);
});

describe("section jumps", () => {
  const model = buildList({
    ...base,
    hits: ["pin.md", "root.md", "Projects/a.md", "Projects/b.md", "Reference/c.md"].map(hit),
    pinned: [item("pin.md")],
  });
  const folders = rows(model, "", new Map(), ["Projects", "Reference", "Empty"]);
  it("jumps from Pinned to the first root note in Notes", () => {
    expect(jumpSection(folders, "pin.md", 1)).toBe("root.md");
  });
  it("skips folder labels and folders without notes", () => {
    expect(jumpSection(folders, "root.md", 1)).toBe("Projects/a.md");
  });
  it("skips the remaining notes of a folder when jumping forward", () => {
    expect(jumpSection(folders, "Projects/a.md", 1)).toBe("Reference/c.md");
  });
  it("jumps backward to the previous section from inside a folder", () => {
    expect(jumpSection(folders, "Projects/b.md", -1)).toBe("root.md");
  });
  it("treats a selected folder label as the start of its own section", () => {
    expect(jumpSection(folders, "folder:Projects", 1)).toBe("Projects/a.md");
    expect(jumpSection(folders, "folder:Projects", -1)).toBe("root.md");
  });
  it("selects the first note of a non-selectable date group", () => {
    const dated = rows(
      model,
      "",
      new Map([
        ["root.md", { key: "today", kind: "today", date: 0 }],
        ["Projects/a.md", { key: "yesterday", kind: "yesterday", date: 0 }],
      ]),
    );
    expect(jumpSection(dated, "root.md", 1)).toBe("Projects/a.md");
  });
  it("does not repeat Notes when its first folder shares the destination", () => {
    const grouped = rows(
      { ...model, notes: model.notes.filter((card) => card.hit.path !== "root.md") },
      "",
      new Map(),
      ["Projects", "Reference"],
    );
    expect(jumpSection(grouped, "pin.md", 1)).toBe("Projects/a.md");
    expect(jumpSection(grouped, "Projects/a.md", -1)).toBe("pin.md");
  });
  it("stays at the first section when jumping above it", () => {
    expect(jumpSection(folders, "pin.md", -1)).toBe("pin.md");
  });
  it("stays at the last section when jumping below it", () => {
    expect(jumpSection(folders, "Reference/c.md", 1)).toBe("Reference/c.md");
  });
  it("starts at the first section with no selection", () => {
    expect(jumpSection(folders, "", 1)).toBe("pin.md");
  });
  it("returns no selection for an empty list", () => {
    expect(jumpSection([], "", 1)).toBe("");
  });
});
