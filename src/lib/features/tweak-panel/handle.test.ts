import { describe, expect, it } from "vitest";
import { exportHandle, handleDefaults, validHandle } from "./handle";

const source = [
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
      source,
    );
    expect(diff.split("\n").filter((line) => /^[-+]pub const/.test(line))).toEqual([
      "-pub const REVEAL_PERCENT: u32 = 400;",
      "+pub const REVEAL_PERCENT: u32 = 250;",
    ]);
  });
});
