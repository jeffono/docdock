import { describe, expect, it } from "vitest";
import { type Effect, type Input, idle, step, type Trigger } from "./trigger";

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

const shown = (thumb: string): Trigger<string> => ({ shown: thumb, waiting: null });

describe("image preview trigger", () => {
  // Catches a preview flashing up while the pointer merely crosses a grid.
  it("shows a hovered thumbnail only after its delay", () => {
    const [state, effects] = run([
      { kind: "hover", thumb: "a" },
      { kind: "elapsed", thumb: "a" },
    ]);
    expect(effects).toEqual(["none", "show"]);
    expect(state).toEqual(shown("a"));
  });

  it("drops a delay that ended after the pointer moved on", () => {
    const [state, effects] = run([
      { kind: "hover", thumb: "a" },
      { kind: "hover", thumb: "b" },
      { kind: "elapsed", thumb: "a" },
    ]);
    expect(effects).toEqual(["none", "none", "none"]);
    expect(state).toEqual({ shown: null, waiting: "b" });
  });

  it("cancels the delay between thumbnails before anything shows", () => {
    const [state] = run([
      { kind: "hover", thumb: "a" },
      { kind: "hover", thumb: null },
    ]);
    expect(state).toEqual(idle());
  });

  it("switches at once to another hovered thumbnail once shown", () => {
    expect(run([{ kind: "hover", thumb: "b" }], shown("a"))).toEqual([shown("b"), ["show"]]);
  });

  it("stays open between thumbnails of the grid", () => {
    expect(run([{ kind: "hover", thumb: null }], shown("a"))).toEqual([shown("a"), ["none"]]);
  });

  it("hides on leaving the grid", () => {
    expect(run([{ kind: "leave" }], shown("a"))).toEqual([idle(), ["hide"]]);
  });

  it("toggles with Space on the focused thumbnail", () => {
    const [state, effects] = run([
      { kind: "toggle", thumb: "a" },
      { kind: "toggle", thumb: "a" },
    ]);
    expect(effects).toEqual(["show", "hide"]);
    expect(state).toEqual(idle());
  });

  it("follows keyboard focus only while shown", () => {
    expect(run([{ kind: "focus", thumb: "b" }], shown("a"))).toEqual([shown("b"), ["show"]]);
    expect(run([{ kind: "focus", thumb: "b" }])).toEqual([idle(), ["none"]]);
  });

  // Catches a stale hover delay showing the preview after a dismissal.
  it("a dismissal hides and cancels the delay", () => {
    expect(run([{ kind: "dismiss" }], shown("a"))).toEqual([idle(), ["hide"]]);
    expect(run([{ kind: "dismiss" }], { shown: null, waiting: "a" })).toEqual([idle(), ["none"]]);
  });

  // Catches Rust being asked to hide again after it already did.
  it("Rust's own hide only resets", () => {
    expect(run([{ kind: "closed" }], shown("a"))).toEqual([idle(), ["none"]]);
  });
});
