import { describe, expect, it } from "vitest";
import { type EditEvent, type EditState, type Effect, reading, step } from "./edit-state";

/** Runs `events` from `state`, answering the state reached and every effect asked on the way. */
function run(state: EditState, ...events: EditEvent[]) {
  const effects: Effect[] = [];
  for (const event of events) {
    const next = step(state, event);
    state = next.state;
    effects.push(...next.effects);
  }
  return { state, effects };
}

const editing = run(reading, { type: "begin" }, { type: "granted" }).state;
const conflict = run(editing, { type: "conflict" }).state;
const parked = run(editing, { type: "parked", dirty: true }).state;
const types = (effects: Effect[]) => effects.map((effect) => effect.type);
/** Effects that drop the editor's text or let the window go. */
const losing = ["unmount", "end", "close", "panel"];

describe("post-it edit state", () => {
  // Catches Esc leaving an edit whose save found the note changed, dropping the user's text.
  it("stays in the editor when leaving meets a conflict", () => {
    const { state, effects } = run(
      editing,
      { type: "leave", after: "read" },
      { type: "flushed", result: "conflict" },
      { type: "leave", after: "read" },
    );
    expect(state.mode).toBe("conflict");
    expect(types(effects).filter((type) => losing.includes(type))).toEqual([]);
  });

  // Catches an exit or folder change going on while the post-it still holds unsaved text.
  it("answers a finish during a conflict as not saved", () => {
    const { state, effects } = run(conflict, { type: "finish", request: 4 });
    expect(effects).toEqual([{ type: "answer", request: 4, saved: false }]);
    expect(state.mode).toBe("conflict");
  });

  // Catches a finish answered as saved when its save failed, ending an edit with text unsaved.
  it("answers a finish with its save's outcome", () => {
    const saved = run(
      editing,
      { type: "finish", request: 2 },
      { type: "flushed", result: "saved" },
    );
    expect(saved.effects).toContainEqual({ type: "answer", request: 2, saved: true });
    expect(types(saved.effects)).not.toContain("end");
    const failed = run(
      editing,
      { type: "finish", request: 3 },
      { type: "flushed", result: "error" },
    );
    expect(failed.effects).toContainEqual({ type: "answer", request: 3, saved: false });
    expect(failed.state.mode).toBe("editing");
  });

  // Catches a change on disk overwriting what the user typed and has not saved yet.
  it("never replaces unsaved text with the note on disk", () => {
    expect(run(editing, { type: "disk", dirty: true }).effects).toEqual([]);
    expect(run(editing, { type: "disk", dirty: false }).effects).toEqual([
      { type: "reload", force: false },
    ]);
  });

  // Catches a rename during a conflict either resuming over a real change elsewhere or staying
  // stuck after a spurious one.
  it("resumes a conflict only while the note keeps the version saves are based on", () => {
    const changed = run(conflict, { type: "disk", dirty: true }, { type: "checked", same: false });
    expect(types(changed.effects)).toEqual(["check"]);
    expect(changed.state.mode).toBe("conflict");
    const same = run(conflict, { type: "disk", dirty: true }, { type: "checked", same: true });
    expect(types(same.effects)).toEqual(["check", "resume"]);
    expect(same.state.mode).toBe("editing");
  });

  // Catches a parked post-it saving into a folder that is gone, or dropping its text.
  it("keeps a parked edit's text and never saves it", () => {
    const { state, effects } = run(
      parked,
      { type: "blur" },
      { type: "leave", after: "read" },
      { type: "leave", after: "close" },
      { type: "finish", request: 5 },
      { type: "disk", dirty: true },
    );
    expect(state.mode).toBe("parked");
    expect(types(effects)).toEqual(["answer"]);
    expect(effects[0]).toEqual({ type: "answer", request: 5, saved: false });
  });

  // Catches a parked post-it with nothing to save waiting forever for its folder.
  it("ends a clean parked edit at once", () => {
    const { effects } = run(editing, { type: "parked", dirty: false });
    expect(types(effects)).toEqual(["unmount", "end"]);
  });

  // Catches a close from outside (Alt+F4, the tray) closing a post-it whose save conflicted.
  it("stays open when asked to close during a conflict", () => {
    const { state, effects } = run(conflict, { type: "leave", after: "close" });
    expect(state.mode).toBe("conflict");
    expect(effects).toEqual([]);
    const closing = run(
      editing,
      { type: "leave", after: "close" },
      { type: "flushed", result: "saved" },
    );
    expect(types(closing.effects)).toEqual(["flush", "unmount", "close"]);
  });
});
