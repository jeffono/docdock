import { describe, expect, it } from "vitest";
import type { ClipboardEntry } from "../../ipc";
import { filterClipboardEntries, previewSnippet } from "./filter";

const entry = (id: string, text: string): ClipboardEntry => ({
  id,
  text,
  truncated: false,
  timestampMs: null,
  deletable: true,
});

describe("clipboard filter", () => {
  it("filters entries case-insensitively and accent-insensitively", () => {
    const items = [
      entry("1", "Ação imediata"),
      entry("2", "Hello world"),
      entry("3", "https://example.com/?utm_source=x"),
    ];
    expect(filterClipboardEntries(items, "acao").map((i) => i.id)).toEqual(["1"]);
    expect(filterClipboardEntries(items, "EXAMPLE").map((i) => i.id)).toEqual(["3"]);
    expect(filterClipboardEntries(items, "   ").length).toBe(3);
  });

  it("collapses whitespace cleanly for card preview snippets", () => {
    expect(previewSnippet("  line 1\n\n  line 2 \t end ")).toBe("line 1 line 2 end");
    expect(previewSnippet(`${"a".repeat(600)}tail`)).toBe("a".repeat(480));
  });
});
