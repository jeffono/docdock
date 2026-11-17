import { describe, expect, it } from "vitest";
import { filterFonts, fontStack, fontsReply } from "./fonts";

describe("installed font stacks", () => {
  it("keeps punctuation inside the family rather than adding CSS families", () => {
    expect(fontStack('A", serif, "B', false)).toBe(
      '"A\\22 , serif, \\22 B", "Segoe UI", system-ui, sans-serif',
    );
  });
  it("escapes backslashes and newlines as CSS escapes", () => {
    expect(fontStack("A\\B\nC", true)).toBe('"A\\5c B\\a C", "Cascadia Mono", Consolas, monospace');
  });
});

describe("fontsReply", () => {
  it("rejects a reply whose families are not a list of names", () => {
    for (const reply of [null, { families: "Arial" }, { families: ["Arial", 3] }]) {
      expect(fontsReply(reply)).toBeInstanceOf(Error);
    }
  });

  it("passes the panel's error on", () => {
    expect(fontsReply({ error: "no fonts" })).toEqual(new Error("no fonts"));
  });
});

it("filters font names case insensitively without matching fallback families", () => {
  expect(filterFonts(['"Segoe UI", system-ui', "Georgia, serif"], "  seGOe ")).toEqual([
    '"Segoe UI", system-ui',
  ]);
  expect(filterFonts(["Georgia, serif"], "serif")).toEqual([]);
});
