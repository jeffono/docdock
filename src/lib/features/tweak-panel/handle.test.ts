import { describe, expect, it } from "vitest";
import { exportHandle, handleDefaults, validHandle } from "./handle";

const size = { depth: 20, height: 152, curve: 28 };

const source = [
  "const DEPTH: u32 = 20;",
  "const HEIGHT: u32 = 152;",
  "const CURVE: u32 = 28;",
  "/// Activation height as a percentage of the tab height.",
  "pub const ACTIVATION_PERCENT: u32 = 300;",
  "/// How near the tab the cursor must come for the handle to show.",
  "pub const REVEAL_PERCENT: u32 = 400;",
  "/// Speed toward the edge from which an approach is a rush.",
  "pub const RUSH_SPEED: u32 = 3000;",
  "",
].join("\n");

describe("edge handle export", () => {
  // Catches handle.rs constants reworded so the export no longer finds them, or a default the
  // tweak window would send that dev_handle rejects.
  it("reads valid release constants from handle.rs", () => {
    expect(validHandle(handleDefaults)).toBe(true);
  });

  it("rewrites only the changed constant", () => {
    const diff = exportHandle(
      { activationPercent: 300, revealPercent: 250, rushSpeed: 3000 },
      size,
      source,
    );
    expect(diff.split("\n").filter((line) => /^[-+]pub const/.test(line))).toEqual([
      "-pub const REVEAL_PERCENT: u32 = 400;",
      "+pub const REVEAL_PERCENT: u32 = 250;",
    ]);
  });

  // Catches two diffs for handle.rs whose shared context lines would make the second fail to apply.
  it("puts the tab size and the handle tuning in one diff", () => {
    const diff = exportHandle(
      { activationPercent: 300, revealPercent: 250, rushSpeed: 3000 },
      { ...size, curve: 30 },
      source,
    );
    expect(diff.match(/^--- /gm)).toHaveLength(1);
    expect(diff.split("\n").filter((line) => /^[-+](pub )?const/.test(line))).toEqual([
      "-const CURVE: u32 = 28;",
      "+const CURVE: u32 = 30;",
      "-pub const REVEAL_PERCENT: u32 = 400;",
      "+pub const REVEAL_PERCENT: u32 = 250;",
    ]);
  });
});
