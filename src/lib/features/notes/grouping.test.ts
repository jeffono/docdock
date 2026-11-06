import { expect, test } from "vitest";
import type { Note, NoteOrder } from "../../ipc";
import { dateHeadings } from "./grouping";
import { buildList, navKeys, rows } from "./list";
import { orderHits } from "./order";
import type { Hit } from "./search";

const hit = (date: Date, path = "note.md"): Hit => ({
  path,
  title: path,
  modified: date.getTime(),
  tags: [],
  snippet: null,
});
const now = new Date(2026, 10, 6, 0, 5);

// These boundaries catch elapsed-hour grouping and overlapping recent/month/year buckets.
test.each([
  [new Date(2026, 10, 6, 0), "today"],
  [new Date(2026, 10, 5, 23, 59, 59, 999), "yesterday"],
  [new Date(2026, 10, 5, 0), "yesterday"],
  [new Date(2026, 10, 4, 23, 59), "week"],
  [new Date(2026, 9, 30, 0), "week"],
  [new Date(2026, 9, 29, 23, 59), "month"],
  [new Date(2026, 9, 7, 0), "month"],
  [new Date(2026, 9, 6, 23, 59), "month:2026:9"],
  [new Date(2026, 8, 30, 23, 59), "month:2026:8"],
  [new Date(2026, 0, 1), "month:2026:0"],
  [new Date(2025, 11, 31, 23, 59), "year:2025"],
] as const)("local date %s belongs to %s", (date, key) => {
  expect(dateHeadings([hit(date)], [], "modified-desc", true, "", now).get("note.md")?.key).toBe(
    key,
  );
});

test("recent groups take priority across the year boundary", () => {
  const hits = [hit(new Date(2025, 11, 31))];
  expect(
    dateHeadings(hits, [], "modified-desc", true, "", new Date(2026, 0, 1)).get("note.md")?.key,
  ).toBe("yesterday");
});

test.each<NoteOrder>(["modified-asc", "modified-desc"])(
  "headers follow %s without reordering notes",
  (order) => {
    const hits = [hit(now, "today"), hit(new Date(2025, 0, 1), "old"), hit(now, "today2")];
    const sorted = orderHits(hits, [], order, "");
    const headers = dateHeadings(sorted, [], order, true, "", now);
    expect([...headers.values()].map((group) => group.key)).toEqual(
      order.endsWith("asc") ? ["year:2025", "today"] : ["today", "year:2025"],
    );
  },
);

test("created ordering groups by birth time rather than modified time", () => {
  const hits = [hit(now)];
  const notes: Note[] = [{ ...hits[0], name: "note.md", created: new Date(2025, 0, 1).getTime() }];
  expect(dateHeadings(hits, notes, "created-desc", true, "", now).get("note.md")?.key).toBe(
    "year:2025",
  );
});

test.each<[NoteOrder, boolean, string]>([
  ["name-asc", true, ""],
  ["name-desc", true, ""],
  ["modified-desc", false, ""],
  ["modified-desc", true, "note"],
])("grouping is inactive for %s, enabled %s, query '%s'", (order, enabled, query) => {
  expect(dateHeadings([hit(now)], [], order, enabled, query, now).size).toBe(0);
});

test("date labels leave pinned rows and edited card keys intact and out of keyboard navigation", () => {
  const model = buildList({
    hits: [hit(now, "pin"), hit(new Date(2025, 0, 1), "old"), hit(now, "today")],
    pinned: [{ target: "pin", title: "Pin", kind: "note", missing: false, collapsed: true }],
    query: "",
    exactTitle: false,
    expanded: "today",
    editing: true,
    kept: null,
  });
  const headers = dateHeadings(
    model.notes.map((card) => card.hit),
    [],
    "modified-asc",
    true,
    "",
    now,
  );
  const flat = rows(model, "today", headers);
  expect(flat.map((row) => row.key)).toEqual([
    "heading:pinned",
    "note:pin",
    "heading:notes",
    "date:year:2025:old",
    "note:old",
    "date:today:today",
    "editing",
  ]);
  expect(navKeys(model)).toEqual(["pin", "old", "today"]);
});
