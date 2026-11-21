import { describe, expect, it } from "vitest";
import { normalizePath } from "./detach";

describe("normalizePath", () => {
  // Catches a floating image's badge staying outlined because the note spells its path otherwise.
  it("compares paths as Rust does", () => {
    for (const path of [
      "attachments/Map.png",
      "./attachments//map.PNG",
      "a/../attachments\\map.png",
    ])
      expect(normalizePath(path)).toBe("attachments/map.png");
  });

  it("gives nothing for a path leaving the folder", () => {
    expect(normalizePath("../map.png")).toBe("");
  });
});
