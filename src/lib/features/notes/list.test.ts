import { describe, expect, it } from "vitest";
import type { PinnedItem } from "../../ipc";
import {
  buildList,
  type Card,
  deeper,
  defaultKey,
  enter,
  isMissing,
  move,
  navKeys,
  noteFileName,
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
  kept: null,
};
const paths = (entries: (Card | { target: string })[]) =>
  entries.map((e) => ("target" in e ? e.target : e.hit.path));

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
    const kept = buildList({ ...base, hits: [], expanded: "x.md", kept: hit("x.md") });
    expect(kept.notes).toMatchObject([{ gone: true, level: "expanded" }]);
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
