import { expect, test } from "vitest";
import {
  applyCompletion,
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
test("explicit creation uses free text and never names a note after filters", () => {
  expect(creationText("budget #work #review")).toBe("budget");
  expect(creationText("#work")).toBe("");
  expect(creationText("#")).toBe("");
  expect(filterQuery("#")).toBe(true);
  expect(filterQuery("budget #work")).toBe(true);
  expect(filterQuery("plain title")).toBe(false);
});
