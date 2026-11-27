import { describe, expect, it } from "vitest";
import { appendDraftEdit, appendDraftText } from "./draft";

describe("appendDraftText", () => {
  it("returns addition directly when draft is empty or whitespace-only", () => {
    expect(appendDraftText("", "clip")).toBe("clip");
    expect(appendDraftText("  \n  ", "clip")).toBe("clip");
  });

  it("appends with one blank line matching existing LF or CRLF endings", () => {
    expect(appendDraftText("first draft\n\n", "second part")).toBe("first draft\n\nsecond part");
    expect(appendDraftText("first\r\nsecond\r\n", "third")).toBe("first\r\nsecond\r\n\r\nthird");
  });

  it("computes minimal end-of-document edits preserving CRLF raw text", () => {
    expect(appendDraftEdit("first\r\nsecond\r\n", "third")).toEqual({
      from: "first\nsecond".length,
      to: "first\nsecond\n".length,
      insert: "\n\nthird",
      nextRaw: "first\r\nsecond\r\n\r\nthird",
    });
    expect(appendDraftEdit("", "fresh\nline")).toEqual({
      from: 0,
      to: 0,
      insert: "fresh\nline",
      nextRaw: "fresh\nline",
    });
  });
});
