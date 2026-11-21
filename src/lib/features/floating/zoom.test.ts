import { describe, expect, it } from "vitest";
import { clampPan, MAX_ZOOM, ZOOM_STEP, zoomAt, zoomStep } from "./zoom";

const natural = { width: 400, height: 200 };
const stage = { width: 400, height: 200 };

describe("zoomStep", () => {
  it("grows the window until it is as large as it gets", () => {
    expect(zoomStep({ width: 200, height: 100 }, natural, 1, false, true)).toEqual({
      kind: "resize",
      width: 200 * ZOOM_STEP,
    });
  });

  // Catches a zoom skipping over the image's natural size, where it is sharpest.
  it("stops at the natural size on the way through", () => {
    expect(zoomStep({ width: 380, height: 190 }, natural, 1, false, true)).toEqual({
      kind: "resize",
      width: 400,
    });
    expect(zoomStep({ width: 420, height: 210 }, natural, 1, false, false)).toEqual({
      kind: "resize",
      width: 400,
    });
  });

  it("zooms inside once the window is as large as it gets, up to the most", () => {
    expect(zoomStep(stage, natural, 1, true, true)).toEqual({ kind: "inner", zoom: ZOOM_STEP });
    expect(zoomStep(stage, natural, MAX_ZOOM, true, true)).toEqual({
      kind: "inner",
      zoom: MAX_ZOOM,
    });
  });

  // Catches zooming out shrinking the window while the image is still zoomed inside it.
  it("returns to no zoom inside before the window shrinks", () => {
    expect(zoomStep(stage, natural, 1.1, true, false)).toEqual({ kind: "inner", zoom: 1 });
    expect(zoomStep(stage, natural, 1, true, false)).toEqual({
      kind: "resize",
      width: 400 / ZOOM_STEP,
    });
  });

  it("grows freely while the natural size is unknown", () => {
    const unknown = { width: 0, height: 0 };
    expect(zoomStep({ width: 300, height: 150 }, unknown, 1, false, false)).toEqual({
      kind: "resize",
      width: 300 / ZOOM_STEP,
    });
  });
});

describe("clampPan", () => {
  // Catches a pan dragging the zoomed image away from the window's edges.
  it("keeps the zoomed image over the whole stage", () => {
    expect(clampPan({ x: 500, y: -500 }, 2, stage)).toEqual({ x: 200, y: -100 });
    expect(clampPan({ x: 50, y: 0 }, 1, stage)).toEqual({ x: 0, y: 0 });
  });

  it("settles on the center when close to it", () => {
    expect(clampPan({ x: 4, y: -3 }, 2, stage)).toEqual({ x: 0, y: 0 });
    expect(clampPan({ x: 7, y: 0 }, 2, stage)).toEqual({ x: 7, y: 0 });
  });
});

describe("zoomAt", () => {
  // Catches zooming toward the pointer drifting the point under it.
  it("keeps the point under the pointer in place", () => {
    const pan = zoomAt({ x: 0, y: 0 }, 1, 2, { x: 100, y: 50 }, stage);
    expect(pan).toEqual({ x: -100, y: -50 });
    // That point is now under the pointer again: center + pan + offset * zoom.
    expect(pan.x + 100 * 2).toBe(100);
  });
});
