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
  it("cycles forward and backward across Notes, Scratchpad, Clipboard and Browser with wrap-around", () => {
    expect(stepTab("notes", 1)).toBe("scratchpad");
    expect(stepTab("scratchpad", 1)).toBe("clipboard");
    expect(stepTab("clipboard", 1)).toBe("browser");
    expect(stepTab("browser", 1)).toBe("notes");
    expect(stepTab("notes", -1)).toBe("browser");
    expect(stepTab("browser", -1)).toBe("clipboard");
    expect(stepTab("clipboard", -1)).toBe("scratchpad");
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
    expect(stepper.step("notes", 20, 100)).toBeNull();
    expect(stepper.step("notes", 20, 120)).toBeNull();
    expect(stepper.step("notes", 20, 140)).toBe("scratchpad");
  });

  it("resets partial accumulation on direction reversal or idle gap", () => {
    const stepper = createWheelStepper(50, 200);
    expect(stepper.step("scratchpad", 30, 200)).toBeNull();
    expect(stepper.step("scratchpad", -30, 210)).toBeNull();
    expect(stepper.step("scratchpad", -30, 220)).toBe("notes");
    expect(stepper.step("notes", 40, 300)).toBeNull();
    expect(stepper.step("notes", 20, 600)).toBeNull();
  });

  it.each([
    ["clipboard", 60],
    ["notes", -60],
  ] as const)("stops at %s before entering Browser in the same gesture", (stop, delta) => {
    const stepper = createWheelStepper();
    expect(stepper.step("scratchpad", delta, 0)).toBe(stop);
    expect(stepper.step(stop, delta, 249)).toBeNull();
  });

  it("allows entry to Browser after 250 ms since the last blocked event", () => {
    const stepper = createWheelStepper();
    expect(stepper.step("scratchpad", 60, 0)).toBe("clipboard");
    expect(stepper.step("clipboard", 60, 100)).toBeNull();
    expect(stepper.step("clipboard", 60, 349)).toBeNull();
    expect(stepper.step("clipboard", 30, 599)).toBeNull();
    expect(stepper.step("clipboard", 20, 620)).toBe("browser");
  });

  it("allows entry to Browser as the first tab step of a gesture", () => {
    const stepper = createWheelStepper();
    expect(stepper.step("notes", -25, 0)).toBeNull();
    expect(stepper.step("notes", -25, 20)).toBe("browser");
  });

  it("allows consecutive steps that do not enter Browser", () => {
    const stepper = createWheelStepper();
    expect(stepper.step("browser", 60, 0)).toBe("notes");
    expect(stepper.step("notes", 60, 20)).toBe("scratchpad");
    expect(stepper.step("scratchpad", 60, 40)).toBe("clipboard");
  });
});
