import { expect, test } from "vitest";
import {
  applyCompletion,
  completionChoices,
  creationText,
  filterQuery,
  parseQuery,
  queryCompletion,
  tagToken,
} from "./query";

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
