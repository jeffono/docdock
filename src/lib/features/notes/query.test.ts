import { expect, test } from "vitest";
import {
  activeFilters,
  applyCompletion,
  clearFilters,
  completionChoices,
  creationContent,
  creationText,
  filterQuery,
  hasActiveFilters,
  noteSuggestionQuery,
  parseQuery,
  queryCompletion,
  setDateFilter,
  tagToken,
  toggleFilterToken,
} from "./query";

test("note searches are ineligible for web suggestions by default", () => {
  expect(noteSuggestionQuery("private project")).toBeNull();
});
test("turning note web suggestions off prevents a query", () => {
  expect(noteSuggestionQuery("private project", false)).toBeNull();
});
test.each([
  [" ab ", null],
  [" abc ", "abc"],
])("requires three characters of free text in %j", (query, expected) => {
  expect(noteSuggestionQuery(query, true)).toBe(expected);
});
test("removes tag filters, including quoted tags", () => {
  expect(noteSuggestionQuery('budget #work #"two words" review', true)).toBe("budget review");
});
test("removes date filters, including invalid dates", () => {
  expect(noteSuggestionQuery("budget date:2026-10..2026-11 date:invalid review", true)).toBe(
    "budget review",
  );
});
test("does not query when only filters remain", () => {
  expect(noteSuggestionQuery('date:today #"two words"', true)).toBeNull();
});
test("does not leak an unfinished quoted tag", () => {
  expect(noteSuggestionQuery('budget #"private draft', true)).toBeNull();
});

test("separates several tag filters from free text with source ranges", () => {
  expect(parseQuery("budget #Work  #review next")).toEqual({
    text: "budget next",
    tokens: [
      { kind: "tag", value: "Work", from: 7, to: 12 },
      { kind: "tag", value: "review", from: 14, to: 21 },
    ],
  });
});
test.each(["#", "# Title", "word#tag", "https://example.com/#tag", "## Title", '#"unfinished'])(
  "keeps literal hashes in %s",
  (query) => {
    expect(parseQuery(query).tokens).toEqual([]);
  },
);
test("completion round trips tags that need quoting", () => {
  for (const tag of ["two words", 'say "hi"', "#literal", "a\\b", "ação"]) {
    expect(parseQuery(tagToken(tag)).tokens[0]?.value).toBe(tag);
  }
});
test("completion replaces the whole token at the caret without losing other filters", () => {
  const query = "budget #worng #review";
  const context = queryCompletion(query, 10);
  expect(context).toEqual({ kind: "tag", from: 7, to: 13, prefix: "wo" });
  if (!context) throw new Error("Missing completion");
  expect(applyCompletion(query, context, "work")).toEqual({
    text: "budget #work #review",
    caret: 13,
  });
});
test("a lone hash offers completion but embedded hashes do not", () => {
  expect(queryCompletion("budget #", 8)?.prefix).toBe("");
  expect(queryCompletion("word#tag", 8)).toBeNull();
  expect(queryCompletion("https://a/#tag", 14)).toBeNull();
});
test.each([
  ["budget #work #review", "budget"],
  ["#work", ""],
  ["#", ""],
])("creation text for %s excludes filters", (query, text) => {
  expect(creationText(query)).toBe(text);
});
test.each([
  ["#", true],
  ["budget #work", true],
  ["plain title", false],
])("filter intent for %s prevents automatic creation", (query, expected) => {
  expect(filterQuery(query)).toBe(expected);
});

test("date completion replaces only its token alongside tags", () => {
  const query = "budget date:7 #work";
  const context = queryCompletion(query, 13);
  expect(context?.kind).toBe("date");
  if (!context) throw new Error("Missing completion");
  expect(applyCompletion(query, context, "7d").text).toBe("budget date:7d #work");
  expect(creationText("budget date:today #work")).toBe("budget");
});

test("a sole completed suggestion leaves note navigation available", () => {
  const context = queryCompletion("#work", 5);
  if (!context) throw new Error("Missing completion");
  expect(completionChoices("#work", context, ["work"])).toEqual([]);
});
test("a partial token still offers its sole matching suggestion", () => {
  const context = queryCompletion("#wor", 4);
  if (!context) throw new Error("Missing completion");
  expect(completionChoices("#wor", context, ["work"])).toEqual(["work"]);
});
test("moving the caret outside a token clears completion", () => {
  expect(queryCompletion("budget #wor", 3)).toBeNull();
});

test("parses and completes is:link and has:gallery filters", () => {
  expect(parseQuery("notes is:link has:gallery #work")).toEqual({
    text: "notes",
    tokens: [
      { kind: "is", value: "link", from: 6, to: 13 },
      { kind: "has", value: "gallery", from: 14, to: 25 },
      { kind: "tag", value: "work", from: 26, to: 31 },
    ],
  });
  const isCtx = queryCompletion("is:li", 5);
  expect(isCtx).toEqual({ kind: "is", from: 0, to: 5, prefix: "li" });
  if (!isCtx) throw new Error("Missing is completion");
  expect(applyCompletion("is:li", isCtx, "link")).toEqual({ text: "is:link ", caret: 8 });
  const hasCtx = queryCompletion("has:gal", 7);
  expect(hasCtx).toEqual({ kind: "has", from: 0, to: 7, prefix: "gal" });
  if (!hasCtx) throw new Error("Missing has completion");
  expect(applyCompletion("has:gal", hasCtx, "gallery")).toEqual({
    text: "has:gallery ",
    caret: 12,
  });
  expect(filterQuery("is:link")).toBe(true);
  expect(filterQuery("has:gallery")).toBe(true);
  expect(creationText("notes is:link has:gallery")).toBe("notes");
  expect(noteSuggestionQuery("notes is:link has:gallery", true)).toBe("notes");
  expect(creationContent("notes is:link has:gallery")).toBe("");
});

test("filter helpers toggle, replace dates, detect active filters and clear cleanly", () => {
  let q = "budget";
  q = toggleFilterToken(q, "is", "link");
  expect(q).toBe("budget is:link");
  q = toggleFilterToken(q, "has", "gallery");
  expect(q).toBe("budget is:link has:gallery");
  q = setDateFilter(q, "today");
  expect(q).toBe("budget is:link has:gallery date:today");
  q = setDateFilter(q, "7d");
  expect(q).toBe("budget is:link has:gallery date:7d");
  expect(setDateFilter(q, "7d")).toBe("budget is:link has:gallery");
  q = toggleFilterToken(q, "tag", "work");
  expect(q).toBe("budget is:link has:gallery date:7d #work");
  expect(activeFilters(q)).toEqual({
    link: true,
    gallery: true,
    dates: ["7d"],
    tagKeys: new Set(["work"]),
  });
  expect(hasActiveFilters(q)).toBe(true);
  expect(hasActiveFilters("budget is:other date:invalid")).toBe(false);
  expect(toggleFilterToken(q, "is", "link")).toBe("budget has:gallery date:7d #work");
  expect(toggleFilterToken(q, "tag", "WORK")).toBe("budget is:link has:gallery date:7d");
  expect(clearFilters(q)).toBe("budget");
});

test("removing a filter preserves internal whitespace in remaining quoted tags", () => {
  expect(toggleFilterToken('#"a  b" is:link', "is", "link")).toBe('#"a  b"');
  expect(setDateFilter('#"a  b"   date:today   budget', null)).toBe('#"a  b" budget');
});
