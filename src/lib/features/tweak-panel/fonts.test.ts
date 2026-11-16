import { describe, expect, it } from "vitest";
import { fontStack } from "./fonts";

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
