import { describe, expect, it } from "vitest";
import { exportMotion, motionDefaults, parseMotions } from "./motion";

const source = [
  "const OPEN: Motion = Motion::new(150, [0.65, 0.0, 0.35, 1.0]);",
  "const CLOSE: Motion = Motion::new(150, [0.65, 0.0, 0.35, 1.0]);",
  "const HANDLE_IN: Motion = Motion::new(150, [0.65, 0.0, 0.35, 1.0]);",
  "const HANDLE_OUT: Motion = Motion::new(150, [0.65, 0.0, 0.35, 1.0]);",
  "",
].join("\n");

describe("motion export", () => {
  // Catches motion.rs constants reworded so the export no longer finds them.
  it("reads every release constant from motion.rs", () => {
    expect(Object.keys(motionDefaults)).toEqual(["open", "close", "handleIn", "handleOut"]);
  });

  it("rewrites only the changed constant as Rust literals", () => {
    const values = parseMotions(source);
    values.close = { duration: 200, curve: [0.34, 1.56, 0.64, 1] };
    const diff = exportMotion(values, source);
    expect(diff.split("\n").filter((line) => /^[-+]const/.test(line))).toEqual([
      "-const CLOSE: Motion = Motion::new(150, [0.65, 0.0, 0.35, 1.0]);",
      "+const CLOSE: Motion = Motion::new(200, [0.34, 1.56, 0.64, 1.0]);",
    ]);
  });
});
