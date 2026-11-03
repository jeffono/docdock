import type { SaveResult } from "../../ipc";

/**
 * - `saved`: the file holds the latest edit.
 * - `pending`: an edit waits for the debounce or for the save in flight.
 * - `saving`: a save is in flight and no newer edit exists.
 * - `conflict`: the file changed outside the app; saving stops until the caller decides.
 * - `error`: the last save failed; the next edit or `flush` retries.
 */
export type AutosaveState = "saved" | "pending" | "saving" | "conflict" | "error";

export type AutosaveOptions = {
  /** Version returned by `notesOpen`. */
  version: string;
  save: (content: string, base: string) => Promise<SaveResult>;
  onState?: (state: AutosaveState, error?: unknown) => void;
  delay?: number;
};

export type Autosave = {
  edit(content: string): void;
  /** Saves now and resolves once nothing is waiting, or saving stopped on a conflict or error. */
  flush(): Promise<AutosaveState>;
  /** Content not yet on disk, such as the edit kept after a conflict. */
  unsaved(): string | null;
};

/** One instance per open note. Each save sends the whole text, so only the latest edit matters. */
export function createAutosave({
  version,
  save,
  onState,
  delay = 1000,
}: AutosaveOptions): Autosave {
  let state: AutosaveState = "saved";
  let pending: string | null = null;
  /** Text of the save in flight, until the file is confirmed to hold it. */
  let sending: string | null = null;
  let inFlight: Promise<void> | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function set(next: AutosaveState, error?: unknown) {
    state = next;
    onState?.(next, error);
  }

  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      // A save in flight reschedules when it ends, and `flush` may have saved everything already.
      if (!inFlight && pending !== null && state !== "conflict") void start();
    }, delay);
  }

  function start(): Promise<void> {
    const content = pending as string;
    pending = null;
    sending = content;
    set("saving");
    const saving = (async () => {
      try {
        const result = await save(content, version);
        if (result.status === "saved") {
          version = result.version;
          set(pending === null ? "saved" : "pending");
        } else {
          // A newer edit already contains everything in `content`.
          pending ??= content;
          set("conflict");
        }
      } catch (error) {
        pending ??= content;
        set("error", error);
      }
      sending = null;
    })();
    // `then` runs after this assignment even if `save` throws synchronously.
    inFlight = saving.then(() => {
      inFlight = null;
      if (state === "pending" && timer === undefined) schedule();
    });
    return inFlight;
  }

  return {
    edit(content) {
      pending = content;
      if (state === "conflict") return;
      set("pending");
      schedule();
    },
    async flush() {
      clearTimeout(timer);
      timer = undefined;
      for (;;) {
        if (inFlight) {
          await inFlight;
          continue;
        }
        if (pending === null || state === "conflict") return state;
        await start();
        if (state === "error") return state;
      }
    },
    unsaved: () => pending ?? sending,
  };
}
