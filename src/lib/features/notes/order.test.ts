import { expect, test } from "vitest";
import type { Note, NoteOrder } from "../../ipc";
import { orderHits } from "./order";
import type { Hit } from "./search";

const hits: Hit[] = [
  { path: "b.md", title: "Alpha", modified: 3, tags: [], snippet: null },
  { path: "a.md", title: "Zulu", modified: 1, tags: [], snippet: null },
  { path: "c.md", title: "Middle", modified: 2, tags: [], snippet: null },
];
const notes: Note[] = hits.map((hit, i) => ({ ...hit, name: hit.title, created: [1, 3, 2][i] }));
test.each<[NoteOrder, string[]]>([
  ["modified-desc", ["b.md", "c.md", "a.md"]],
  ["modified-asc", ["a.md", "c.md", "b.md"]],
  ["created-desc", ["a.md", "c.md", "b.md"]],
  ["created-asc", ["b.md", "c.md", "a.md"]],
  ["name-asc", ["b.md", "c.md", "a.md"]],
  ["name-desc", ["a.md", "c.md", "b.md"]],
])("unfiltered order %s", (order, paths) => {
  expect(orderHits(hits, notes, order, "").map((hit) => hit.path)).toEqual(paths);
});
test("search retains relevance regardless of the saved order", () => {
  expect(orderHits(hits, notes, "name-desc", "query")).toBe(hits);
});
test("equal dates keep the existing path tiebreaker", () => {
  expect(
    orderHits(
      hits.map((hit) => ({ ...hit, modified: 1 })),
      notes,
      "modified-desc",
      "",
    ).map((hit) => hit.path),
  ).toEqual(["a.md", "b.md", "c.md"]);
});
