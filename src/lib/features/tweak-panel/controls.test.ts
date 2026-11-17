import { describe, expect, it } from "vitest";
import { hsbToRgb, type RGB, rgbToHsb, stepValue } from "./controls";

describe("HSB conversion", () => {
  it("handles black and gray without undefined hue or saturation", () => {
    expect(rgbToHsb([0, 0, 0])).toEqual([0, 0, 0]);
    expect(rgbToHsb([128, 128, 128])).toEqual([0, 0, (128 / 255) * 100]);
  });
  it("maps hue sectors and wraps the hue boundary", () => {
    const primaries: RGB[] = [
      [255, 0, 0],
      [255, 255, 0],
      [0, 255, 0],
      [0, 255, 255],
      [0, 0, 255],
      [255, 0, 255],
    ];
    primaries.forEach((rgb, i) => {
      expect(hsbToRgb([i * 60, 100, 100])).toEqual(rgb);
    });
    expect(hsbToRgb([360, 100, 100])).toEqual(primaries[0]);
  });
  it("round trips sRGB channels without color loss", () => {
    for (let r = 0; r <= 255; r += 51)
      for (let g = 0; g <= 255; g += 51)
        for (let b = 0; b <= 255; b += 51) {
          const rgb: RGB = [r, g, b];
          expect(hsbToRgb(rgbToHsb(rgb))).toEqual(rgb);
        }
  });
});
describe("numeric stepping", () => {
  it("uses decimal steps without accumulating rounding errors", () => {
    expect(stepValue(0.1, 1, 0.01, 0, 1)).toBe(0.11);
  });
  it("multiplies the step with Shift", () => {
    expect(stepValue(20, -1, 1, 0, 100, true)).toBe(10);
  });
  it("clamps both range boundaries", () => {
    expect(stepValue(99, 1, 10, 0, 100)).toBe(100);
    expect(stepValue(1, -1, 10, 0, 100)).toBe(0);
  });
});
