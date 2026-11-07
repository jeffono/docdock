import { expect, test } from "vitest";
import { dragSlot } from "./drag";

test("a slow drag stays still until the next slot center is crossed", () => {
  for (const y of [20, 60, 100, 149]) expect(dragSlot([100, 100, 100], 0, 0, y)).toBe(0);
  expect(dragSlot([100, 100, 100], 0, 0, 151)).toBe(1);
});

test("stationary pointer cannot reverse a swap while unequal cards animate", () => {
  expect(dragSlot([160, 60, 100], 0, 0, 191)).toBe(1);
  for (let frame = 0; frame < 20; frame++) expect(dragSlot([160, 60, 100], 0, 1, 191)).toBe(1);
  expect(dragSlot([160, 60, 100], 0, 1, 29)).toBe(0);
});

test("upward movement uses the current preview layout", () => {
  expect(dragSlot([100, 60, 160], 2, 2, 129)).toBe(1);
  expect(dragSlot([100, 60, 160], 2, 1, 129)).toBe(1);
  expect(dragSlot([100, 60, 160], 2, 1, 49)).toBe(0);
});
