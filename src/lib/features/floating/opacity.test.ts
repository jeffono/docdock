import { describe, expect, it } from "vitest";
import { MIN_OPACITY, stepOpacity } from "./opacity";

describe("stepOpacity", () => {
  // Catches binary rounding leaving the wheel off the slider's grid, so 100% is never reached.
  it("stays on whole steps", () => {
    let opacity = MIN_OPACITY;
    for (let step = 0; step < 17; step++) opacity = stepOpacity(opacity, true);
    expect(opacity).toBe(1);
    expect(stepOpacity(0.33, false)).toBe(0.3);
  });

  it("never leaves the bounds", () => {
    expect(stepOpacity(1, true)).toBe(1);
    expect(stepOpacity(MIN_OPACITY, false)).toBe(MIN_OPACITY);
  });
});
