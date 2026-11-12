import { describe, expect, it } from "vitest";
import { folderAction } from "./folder-state";

describe("folderAction", () => {
  it("never drops an editor holding unsaved text when the folder leaves", () => {
    expect(folderAction("available", "unavailable", true, true)).toBe("keep");
    expect(folderAction("available", "missing", true, true)).toBe("keep");
    expect(folderAction("unavailable", "unavailable", true, true)).toBe("keep");
  });

  it("forgets everything when the folder leaves with nothing unsaved", () => {
    expect(folderAction("available", "unavailable", true, false)).toBe("close");
    expect(folderAction("available", "missing", false, false)).toBe("close");
  });

  it("saves the kept text when the folder returns during editing", () => {
    expect(folderAction("unavailable", "available", true, true)).toBe("resume");
    expect(folderAction("missing", "available", true, true)).toBe("resume");
  });

  it("starts over when the folder returns without an editor", () => {
    expect(folderAction("unavailable", "available", false, false)).toBe("open");
    expect(folderAction(null, "available", false, false)).toBe("open");
  });

  it("rereads in place while the folder stays available", () => {
    expect(folderAction("available", "available", true, true)).toBe("reload");
    expect(folderAction("available", "available", false, false)).toBe("reload");
  });
});
