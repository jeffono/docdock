import { describe, expect, it } from "vitest";
import { type Effect, type Input, idle, step, type Target, type Trigger } from "./trigger";

function run(inputs: Input<string>[], from: Trigger<string> = idle()): [Trigger<string>, Effect[]] {
  const effects: Effect[] = [];
  let state = from;
  for (const input of inputs) {
    const [next, effect] = step(state, input);
    state = next;
    effects.push(effect);
  }
  return [state, effects];
}

const img = (thumb: string): Target<string> => ({ kind: "image", item: thumb });
const note = (path: string): Target<string> => ({ kind: "note", item: path });

const shownImage = (thumb: string): Trigger<string> => ({
  ...idle(),
  shown: img(thumb),
});

describe("image preview trigger", () => {
  // Catches a preview flashing up while the pointer merely crosses a grid.
  it("shows a hovered thumbnail only after its delay", () => {
    const [state, effects] = run([
      { kind: "hover", thumb: "a" },
      { kind: "elapsed", target: img("a") },
    ]);
    expect(effects).toEqual(["none", "show"]);
    expect(state.shown).toEqual(img("a"));
    expect(state.waiting).toBeNull();
  });

  it("drops a delay that ended after the pointer moved on", () => {
    const [state, effects] = run([
      { kind: "hover", thumb: "a" },
      { kind: "hover", thumb: "b" },
      { kind: "elapsed", target: img("a") },
    ]);
    expect(effects).toEqual(["none", "none", "none"]);
    expect(state.shown).toBeNull();
    expect(state.waiting).toEqual({ ...img("b"), delay: "hover" });
  });

  it("cancels the delay between thumbnails before anything shows", () => {
    const [state] = run([
      { kind: "hover", thumb: "a" },
      { kind: "hover", thumb: null },
    ]);
    expect(state).toEqual(idle());
  });

  it("switches at once to another hovered thumbnail once shown", () => {
    const [state, effects] = run([{ kind: "hover", thumb: "b" }], shownImage("a"));
    expect(effects).toEqual(["show"]);
    expect(state.shown).toEqual(img("b"));
  });

  it("stays open between thumbnails of the grid", () => {
    const [state, effects] = run([{ kind: "hover", thumb: null }], shownImage("a"));
    expect(effects).toEqual(["none"]);
    expect(state.shown).toEqual(img("a"));
  });

  it("hides on leaving the grid", () => {
    const [state, effects] = run([{ kind: "leave" }], shownImage("a"));
    expect(effects).toEqual(["hide"]);
    expect(state).toEqual(idle());
  });

  it("toggles with Space on the focused thumbnail even when note preview is off", () => {
    const [state, effects] = run(
      [
        { kind: "noteMode", mode: "off" },
        { kind: "toggle", thumb: "a" },
        { kind: "toggle", thumb: "a" },
      ],
      idle(),
    );
    expect(effects).toEqual(["none", "show", "hide"]);
    expect(state.shown).toBeNull();
  });

  it("follows keyboard focus only while shown", () => {
    const [switched, eff1] = run([{ kind: "focus", thumb: "b" }], shownImage("a"));
    expect(eff1).toEqual(["show"]);
    expect(switched.shown).toEqual(img("b"));
    const [ignored, eff2] = run([{ kind: "focus", thumb: "b" }]);
    expect(eff2).toEqual(["none"]);
    expect(ignored.shown).toBeNull();
  });

  // Catches a stale hover delay showing the preview after a dismissal.
  it("a dismissal hides and cancels the delay", () => {
    expect(run([{ kind: "dismiss" }], shownImage("a"))[1]).toEqual(["hide"]);
    const [cancelled, effects] = run([{ kind: "dismiss" }], {
      ...idle(),
      waiting: { ...img("a"), delay: "hover" },
    });
    expect(effects).toEqual(["none"]);
    expect(cancelled.waiting).toBeNull();
  });

  // Catches Rust being asked to hide again after it already did.
  it("Rust's own hide only resets", () => {
    const [state, effects] = run([{ kind: "closed" }], shownImage("a"));
    expect(effects).toEqual(["none"]);
    expect(state).toEqual(idle());
  });
});

describe("closed note preview trigger", () => {
  it("shows after hover delay and debounces switches between notes in hover mode", () => {
    const [state, effects] = run([
      { kind: "hoverNote", note: "a.md" },
      { kind: "elapsed", target: note("a.md") },
      { kind: "hoverNote", note: "b.md" },
      { kind: "elapsed", target: note("b.md") },
      { kind: "hoverNote", note: null },
    ]);
    expect(effects).toEqual(["none", "show", "none", "show", "hide"]);
    expect(state.shown).toBeNull();
  });

  it("does not preview closed notes when disabled, while keeping image previews active", () => {
    const [state, effects] = run([
      { kind: "noteMode", mode: "off" },
      { kind: "hoverNote", note: "a.md" },
      { kind: "selectNote", note: "b.md" },
      { kind: "hover", thumb: "pic.png" },
      { kind: "elapsed", target: img("pic.png") },
    ]);
    expect(effects).toEqual(["none", "none", "none", "none", "show"]);
    expect(state.shown).toEqual(img("pic.png"));
  });

  it("in alt mode, opens immediately when Alt arrives over a hovered note and hides on release", () => {
    const [state, effects] = run(
      [
        { kind: "hoverNote", note: "a.md" },
        { kind: "alt", held: true },
        { kind: "alt", held: false },
      ],
      idle("alt"),
    );
    expect(effects).toEqual(["none", "show", "hide"]);
    expect(state.hoveredNote).toBe("a.md");
    expect(state.shown).toBeNull();
  });

  it("in alt mode, switches between hovered notes while Alt stays held", () => {
    const [state, effects] = run(
      [
        { kind: "alt", held: true },
        { kind: "hoverNote", note: "a.md" },
        { kind: "hoverNote", note: "b.md" },
        { kind: "elapsed", target: note("b.md") },
      ],
      idle("alt"),
    );
    expect(effects).toEqual(["none", "show", "none", "show"]);
    expect(state.shown).toEqual(note("b.md"));
  });

  it("hides note preview immediately when dragging starts", () => {
    const [state, effects] = run([
      { kind: "hoverNote", note: "a.md" },
      { kind: "elapsed", target: note("a.md") },
      { kind: "dragging", active: true },
      { kind: "hoverNote", note: "b.md" },
    ]);
    expect(effects).toEqual(["none", "show", "hide", "none"]);
    expect(state.shown).toBeNull();
    expect(state.dragging).toBe(true);
  });

  it("in alt mode, Alt over a focused note shows it immediately and follows selection across hovered pointers", () => {
    const [state, effects] = run(
      [
        { kind: "hoverNote", note: "hovered.md" },
        { kind: "alt", held: true, selected: "focused.md" },
        { kind: "selectNote", note: "next.md" },
        { kind: "elapsed", target: note("next.md") },
        { kind: "alt", held: false },
      ],
      idle("alt"),
    );
    expect(effects).toEqual(["none", "show", "none", "show", "hide"]);
    expect(state.selectedNote).toBe("next.md");
    expect(state.shown).toBeNull();

    const [focusOnly, focusEffects] = run(
      [
        { kind: "alt", held: true, selected: "focused.md" },
        { kind: "alt", held: false },
      ],
      idle("alt"),
    );
    expect(focusEffects).toEqual(["show", "hide"]);
    expect(focusOnly.selectedNote).toBe("focused.md");
    expect(focusOnly.shown).toBeNull();
  });

  it("clears hoveredNote when entering an image grid gap so Alt does not reopen the old note", () => {
    const [state, effects] = run(
      [
        { kind: "hoverNote", note: "old.md" },
        { kind: "hover", thumb: null },
        { kind: "alt", held: true, selected: null },
      ],
      idle("alt"),
    );
    expect(effects).toEqual(["none", "none", "none"]);
    expect(state.hoveredNote).toBeNull();
    expect(state.shown).toBeNull();
  });
});
