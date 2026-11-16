import { describe, expect, it } from "vitest";
import { fontFamilies } from "./handle-look";

describe("fontFamilies", () => {
  // Catches the native handle asking GDI for `system-ui`, which it would replace with any font.
  it("keeps named families in order without quotes or generic families", () => {
    expect(fontFamilies(`"Segoe UI Variable Text", 'Segoe UI', system-ui, Sans-Serif`)).toEqual([
      "Segoe UI Variable Text",
      "Segoe UI",
    ]);
  });
});
