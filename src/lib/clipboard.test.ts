import { expect, test } from "vitest";
import { inOrder } from "./clipboard";

test("a later write waits for an earlier one still retrying", async () => {
  const landed: string[] = [];
  let release = () => {};
  const busy = new Promise<void>((resolve) => {
    release = resolve;
  });
  const write = inOrder(async (text) => {
    if (text === "copy") await busy;
    landed.push(text);
  });
  const copy = write("copy");
  const cut = write("cut");
  release();
  await Promise.all([copy, cut]);
  expect(landed).toEqual(["copy", "cut"]);
});
