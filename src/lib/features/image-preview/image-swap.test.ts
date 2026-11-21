import { describe, expect, it } from "vitest";
import { imageSwap } from "./image-swap";

/** An image whose decode waits until the test ends it. */
function image() {
  let finish = () => {};
  const decoding = new Promise<void>((resolve) => {
    finish = resolve;
  });
  return { src: "", decode: () => decoding, finish };
}

function setup() {
  const created: ReturnType<typeof image>[] = [];
  const mounted: string[] = [];
  let unmounted = 0;
  const show = imageSwap(
    () => {
      const next = image();
      created.push(next);
      return next;
    },
    (shown) => mounted.push(shown.src),
    () => {
      unmounted++;
    },
  );
  return { show, created, mounted, unmounted: () => unmounted };
}

describe("image swap", () => {
  it("mounts an image once it is decoded", async () => {
    const { show, created, mounted } = setup();
    const done = show("a.png");
    expect(mounted).toEqual([]);
    created[0].finish();
    await done;
    expect(mounted).toEqual(["a.png"]);
  });

  // Catches a note image kept in memory, or shown, after the preview hid mid-decode.
  it("hiding during a decode clears its source and never mounts it", async () => {
    const { show, created, mounted, unmounted } = setup();
    const first = show("a.png");
    await show(null);
    expect(created[0].src).toBe("");
    expect(unmounted()).toBe(1);
    created[0].finish();
    await first;
    expect(mounted).toEqual([]);
  });

  it("a later image replaces one still decoding", async () => {
    const { show, created, mounted } = setup();
    const first = show("a.png");
    const second = show("b.png");
    expect(created[0].src).toBe("");
    created[0].finish();
    created[1].finish();
    await Promise.all([first, second]);
    expect(mounted).toEqual(["b.png"]);
  });
});
