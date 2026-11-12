import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SaveResult } from "../../ipc";
import { type AutosaveState, createAutosave } from "./autosave";

type Call = {
  content: string;
  base: string;
  resolve: (result: SaveResult) => void;
  reject: (error: unknown) => void;
};

function setup() {
  const calls: Call[] = [];
  let state: AutosaveState = "saved";
  const autosave = createAutosave({
    version: "v0",
    delay: 100,
    save: (content, base) =>
      new Promise((resolve, reject) => calls.push({ content, base, resolve, reject })),
    onState: (next) => {
      state = next;
    },
  });
  return { autosave, calls, state: () => state };
}

const wait = (ms = 0) => vi.advanceTimersByTimeAsync(ms);

describe("autosave", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("saves only the latest edit after the pause", async () => {
    const { autosave, calls } = setup();
    autosave.edit("a");
    await wait(60);
    autosave.edit("ab");
    await wait(60);
    expect(calls).toHaveLength(0);
    await wait(40);
    expect(calls.map((c) => [c.content, c.base])).toEqual([["ab", "v0"]]);
  });

  it("keeps one save in flight and saves later edits on the new version", async () => {
    const { autosave, calls, state } = setup();
    autosave.edit("a");
    await wait(100);
    autosave.edit("ab");
    await wait(100);
    expect(calls).toHaveLength(1);
    calls[0].resolve({ status: "saved", version: "v1" });
    await wait(100);
    expect(calls.map((c) => [c.content, c.base])).toEqual([
      ["a", "v0"],
      ["ab", "v1"],
    ]);
    calls[1].resolve({ status: "saved", version: "v2" });
    await wait();
    expect(state()).toBe("saved");
    expect(autosave.unsaved()).toBeNull();
  });

  it("reports the text in flight as unsaved until the save is confirmed", async () => {
    const { autosave, calls } = setup();
    autosave.edit("a");
    await wait(100);
    expect(calls).toHaveLength(1);
    expect(autosave.unsaved()).toBe("a");
    calls[0].resolve({ status: "saved", version: "v1" });
    await wait();
    expect(autosave.unsaved()).toBeNull();
  });

  it("flush waits for the save in flight and saves the queued edit", async () => {
    const { autosave, calls } = setup();
    autosave.edit("a");
    await wait(100);
    autosave.edit("ab");
    const flushed = autosave.flush();
    calls[0].resolve({ status: "saved", version: "v1" });
    await wait();
    expect(calls[1]).toMatchObject({ content: "ab", base: "v1" });
    calls[1].resolve({ status: "saved", version: "v2" });
    expect(await flushed).toBe("saved");
    await wait(1000);
    expect(calls).toHaveLength(2);
  });

  it("stops on a conflict and keeps the latest edit", async () => {
    const { autosave, calls, state } = setup();
    autosave.edit("a");
    await wait(100);
    autosave.edit("ab");
    calls[0].resolve({ status: "conflict" });
    await wait();
    expect(state()).toBe("conflict");
    autosave.edit("abc");
    await wait(1000);
    expect(await autosave.flush()).toBe("conflict");
    expect(calls).toHaveLength(1);
    expect(autosave.unsaved()).toBe("abc");
  });

  it("keeps the edit after a failed save and retries on flush", async () => {
    const { autosave, calls, state } = setup();
    autosave.edit("a");
    await wait(100);
    calls[0].reject(new Error("locked"));
    await wait();
    expect(state()).toBe("error");
    expect(autosave.unsaved()).toBe("a");
    const flushed = autosave.flush();
    await wait();
    expect(calls[1]).toMatchObject({ content: "a", base: "v0" });
    calls[1].resolve({ status: "saved", version: "v1" });
    expect(await flushed).toBe("saved");
    expect(autosave.unsaved()).toBeNull();
  });

  it("sends again a save in flight that fails after a retry", async () => {
    const { autosave, calls, state } = setup();
    autosave.edit("a");
    await wait(100);
    autosave.retry();
    calls[0].reject({ code: "folderUnavailable" });
    await wait();
    expect(calls[1]).toMatchObject({ content: "a", base: "v0" });
    calls[1].resolve({ status: "saved", version: "v1" });
    await wait();
    expect(state()).toBe("saved");
    expect(autosave.unsaved()).toBeNull();
  });

  it("retries a failed save at once", async () => {
    const { autosave, calls, state } = setup();
    autosave.edit("a");
    await wait(100);
    calls[0].reject({ code: "folderUnavailable" });
    await wait();
    autosave.retry();
    await wait();
    expect(calls[1]).toMatchObject({ content: "a", base: "v0" });
    calls[1].resolve({ status: "saved", version: "v1" });
    await wait();
    expect(state()).toBe("saved");
  });

  it("never saves edits after stop, even with a save in flight", async () => {
    const { autosave, calls } = setup();
    autosave.edit("a");
    await wait(100);
    autosave.edit("b");
    autosave.stop();
    autosave.edit("c");
    calls[0].resolve({ status: "saved", version: "v1" });
    await wait(500);
    await autosave.flush();
    expect(calls.map((call) => call.content)).toEqual(["a"]);
    expect(autosave.unsaved()).toBeNull();
  });
});
