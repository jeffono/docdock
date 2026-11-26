import { describe, expect, it } from "vitest";
import type { NotesFolder } from "../../ipc";
import { scratchpadSyncAction } from "./folder-sync";

const folder = (state: NotesFolder["state"], path = "C:\\Notes"): NotesFolder => ({
  path,
  state,
});

describe("scratchpadSyncAction", () => {
  it("resumes and preserves unsaved text when trusting an unrecognized folder from Notes", () => {
    expect(
      scratchpadSyncAction(folder("unrecognized"), "unrecognized", folder("available"), true, true),
    ).toBe("resume");
  });

  it("resumes when a write failed with unrecognized before watcher refreshed folder state", () => {
    expect(
      scratchpadSyncAction(folder("available"), "unrecognized", folder("available"), true, true),
    ).toBe("resume");
  });

  it("reloads in place without unmounting the editor when re-checking the same available folder", () => {
    expect(scratchpadSyncAction(folder("available"), null, folder("available"), true, false)).toBe(
      "reload",
    );
  });

  it("switches to a different folder path cleanly", () => {
    expect(
      scratchpadSyncAction(
        folder("available", "C:\\Notes"),
        null,
        folder("available", "C:\\Other"),
        true,
        false,
      ),
    ).toBe("switch");
  });

  it("keeps unsaved text in memory when the folder leaves while dirty", () => {
    expect(scratchpadSyncAction(folder("available"), null, folder("unavailable"), true, true)).toBe(
      "keep",
    );
  });

  it("closes the editor when the folder leaves without unsaved changes", () => {
    expect(
      scratchpadSyncAction(folder("available"), null, folder("unavailable"), true, false),
    ).toBe("close");
  });
});
