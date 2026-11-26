import { describe, expect, it } from "vitest";
import { createWheelStepper, stepTab, wheelTabDelta } from "./panel-tabs";

const wheel = (overrides: Partial<Parameters<typeof wheelTabDelta>[1]> = {}) => ({
  deltaX: 0,
  deltaY: 0,
  shiftKey: false,
  ctrlKey: false,
  altKey: false,
  metaKey: false,
  ...overrides,
});

describe("panel tabs", () => {
  it("cycles forward and backward across Notes, Scratchpad and Browser with wrap-around", () => {
    expect(stepTab("notes", 1)).toBe("scratchpad");
    expect(stepTab("scratchpad", 1)).toBe("browser");
    expect(stepTab("browser", 1)).toBe("notes");
    expect(stepTab("notes", -1)).toBe("browser");
    expect(stepTab("browser", -1)).toBe("scratchpad");
    expect(stepTab("scratchpad", -1)).toBe("notes");
  });

  it("ignores wheel events when mode is off or unsupported modifiers are held", () => {
    expect(wheelTabDelta("off", wheel({ deltaY: 120, shiftKey: true }))).toBe(0);
    expect(wheelTabDelta("on", wheel({ deltaY: 100, ctrlKey: true }))).toBe(0);
    expect(wheelTabDelta("on", wheel({ deltaY: 100, altKey: true }))).toBe(0);
    expect(wheelTabDelta("on", wheel({ deltaY: 100, metaKey: true }))).toBe(0);
  });

  it("requires Shift in modifier mode and maps Windows Shift+wheel deltaX", () => {
    expect(wheelTabDelta("modifier", wheel({ deltaY: 120 }))).toBe(0);
    expect(wheelTabDelta("modifier", wheel({ deltaY: 120, shiftKey: true }))).toBe(120);
    expect(wheelTabDelta("modifier", wheel({ deltaX: -80, shiftKey: true }))).toBe(-80);
  });

  it("accepts unmodified wheel deltas in on mode", () => {
    expect(wheelTabDelta("on", wheel({ deltaY: -100 }))).toBe(-100);
  });

  it("accumulates small touchpad deltas across the switch threshold", () => {
    const stepper = createWheelStepper(50, 200);
    expect(stepper.step(20, 100)).toBeNull();
    expect(stepper.step(20, 120)).toBeNull();
    expect(stepper.step(20, 140)).toBe(1);
  });

  it("resets partial accumulation on direction reversal or idle gap", () => {
    const stepper = createWheelStepper(50, 200);
    expect(stepper.step(30, 200)).toBeNull();
    expect(stepper.step(-30, 210)).toBeNull();
    expect(stepper.step(-30, 220)).toBe(-1);
    expect(stepper.step(40, 300)).toBeNull();
    expect(stepper.step(20, 600)).toBeNull();
  });
});
