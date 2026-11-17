import { describe, expect, it } from "vitest";
import { exportWordmark, parseTab, tabDefaults, validTab } from "./tab-shape";

describe("tab shape", () => {
  // Catches handle.rs or EdgeTab.svelte constants reworded so the export no longer finds them, or a
  // default the tweak window would send that dev_tab_shape rejects.
  it("reads valid release constants from handle.rs and EdgeTab.svelte", () => {
    expect(validTab(tabDefaults)).toBe(true);
  });

  it("refuses a source whose constant is spelled differently", () => {
    expect(() => parseTab("pub const DEPTH: u32 = 20;", "const WORDMARK = 52;")).toThrow(/DEPTH/);
  });

  // Catches curves that overlap, whose outline the window region cannot follow.
  it("refuses curves taller than half the tab", () => {
    expect(validTab({ ...tabDefaults, height: 100, curve: 50 })).toBe(true);
    expect(validTab({ ...tabDefaults, height: 100, curve: 51 })).toBe(false);
  });

  it("rewrites only the wordmark constant in EdgeTab.svelte", () => {
    const source = [
      "/** Length of the wordmark along the tab. */",
      "const WORDMARK = 52;",
      "",
    ].join("\n");
    const diff = exportWordmark(60, source);
    expect(diff).toContain("+++ b/src/lib/features/tab/EdgeTab.svelte");
    expect(diff.split("\n").filter((line) => /^[-+]const/.test(line))).toEqual([
      "-const WORDMARK = 52;",
      "+const WORDMARK = 60;",
    ]);
  });
});
