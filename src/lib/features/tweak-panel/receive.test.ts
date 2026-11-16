import { describe, expect, it } from "vitest";
import { handleDefaults } from "./handle";
import { motionDefaults } from "./motion";
import { receive } from "./receive";

describe("receive", () => {
  it("drops ordinary CSS properties, which would restyle the panel root", () => {
    expect(receive({ tokens: { display: "none" } }).tokens).toEqual({});
  });

  it("falls back to the source values for a malformed event", () => {
    for (const event of [null, "x", { tokens: null, motion: 3, handle: [] }]) {
      expect(receive(event)).toEqual({
        tokens: {},
        motion: motionDefaults,
        handle: handleDefaults,
      });
    }
  });

  it("keeps the source motion where a curve is invalid", () => {
    const motion = { ...motionDefaults, open: { duration: 150, curve: [2, 0, 0, 1] } };
    expect(receive({ motion }).motion.open).toEqual(motionDefaults.open);
  });
});
