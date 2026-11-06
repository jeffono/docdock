import { expect, it } from "vitest";
import { barDefault, historyEntries, LatestQuery, webEntries, webQuery } from "./suggestions";

it.each(["todo.md", "file.txt", "1.5"])("Enter preserves the note match for %s", (query) => {
  const items = webEntries(query, false, [], []);
  expect(items[0]?.kind).toBe("open");
  expect(barDefault(query, false, "note.md", items)).toBe("note.md");
});
it.each(["todo.md", "file.txt", "1.5"])(
  "Enter preserves Create for %s without matches",
  (query) => {
    expect(barDefault(query, false, "create", webEntries(query, false, [], []))).toBe("create");
  },
);
it("selects Open by default only for an explicit web address in notes", () => {
  const query = "https://example.com";
  expect(barDefault(query, false, "create", webEntries(query, false, [], []))).toBe("web:open");
});
it("ordinary note searches cannot supply a remote query", () => {
  expect(webQuery("private project", false)).toBeNull();
  expect(webEntries("private project", false, [], ["remote"])).toEqual([
    { key: "web:search", kind: "intent", text: "private project" },
  ]);
});
it("explicit intent offers local addresses before engine suggestions", () => {
  const items = webEntries("? rust", false, ["https://rust-lang.org/"], ["rust book"]);
  expect(items.map((item) => item.text)).toEqual(["rust", "https://rust-lang.org/", "rust book"]);
  expect(webQuery("? rust", false)).toBe("rust");
});
it("browser Enter selects navigation for URLs and search for text", () => {
  expect(webEntries("example.com", true, [], [])[0]?.kind).toBe("open");
  const items = webEntries("rust book", true, [], []);
  expect(items[0]?.kind).toBe("search");
  expect(barDefault("rust book", true, "", items)).toBe("web:search");
});
it("discards responses superseded by another query, even after returning to the original text", () => {
  const latest = new LatestQuery();
  const first = latest.next();
  latest.next();
  const current = latest.next();
  expect(latest.accepts(first)).toBe(false);
  expect(latest.accepts(current)).toBe(true);
});

it("history opening returns recent local addresses without a query or engine suggestions", () => {
  const history = [
    "https://example.com/",
    "https://duckduckgo.com/?q=rust",
    "https://a.test/",
    "https://b.test/",
    "https://c.test/",
    "https://d.test/",
  ];
  expect(historyEntries(history).map((item) => item.url)).toEqual(history.slice(0, 5));
  expect(historyEntries([])).toEqual([]);
});
