import { describe, expect, it } from "vitest";
import { exportHandle, handleDefaults, validHandle } from "./handle";

const source = [
  "/// Activation height as a percentage of the tab height.",
  "pub const ACTIVATION_PERCENT: u32 = 300;",
  "/// How near the edge the cursor must come for the handle to show.",
  "pub const REVEAL_DISTANCE: u32 = 48;",
  "",
].join("\n");

describe("edge handle export", () => {
  // Catches handle.rs constants reworded so the export no longer finds them, or a default the
  // tweak window would send that dev_handle rejects.
  it("reads valid release constants from handle.rs", () => {
    expect(validHandle(handleDefaults)).toBe(true);
  });

  it("rewrites only the changed constant", () => {
    const diff = exportHandle({ activationPercent: 300, revealDistance: 64 }, source);
    expect(diff.split("\n").filter((line) => /^[-+]pub const/.test(line))).toEqual([
      "-pub const REVEAL_DISTANCE: u32 = 48;",
      "+pub const REVEAL_DISTANCE: u32 = 64;",
    ]);
  });
});
