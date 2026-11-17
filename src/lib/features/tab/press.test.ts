import { describe, expect, it } from "vitest";
import { Press } from "./press";

const slop = [4, 4] as const;

describe("Press", () => {
  // Catches a drag that comes back before the release closing the panel.
  it("is no click after dragging away and back", () => {
    const press = new Press(10, 40);
    press.moved(10, 90, slop);
    press.moved(10, 40, slop);
    expect(press.isClick(10, 40, slop)).toBe(false);
  });

  it("is a click within the drag tolerance", () => {
    expect(new Press(10, 40).isClick(14, 36, slop)).toBe(true);
  });
});
