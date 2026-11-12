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
  /**
   * Saves what is not on disk after a failure, such as the notes folder coming back. A save in flight
   * may have started before it came back: if that one fails, it is sent again at once.
   */
  retry(): void;
  /** Content not yet on disk, such as the edit kept after a conflict. */
  unsaved(): string | null;
  /** Drops what is not on disk and ignores later edits; a save in flight still completes. */
  stop(): void;
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
  let stopped = false;
  /** `retry` came while a save was in flight: a failure of that save sends it again. */
  let retrying = false;
  /** The save in flight failed after `retry`: start again once it is done. */
  let again = false;

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
        if (stopped) return;
        if (result.status === "saved") {
          version = result.version;
          set(pending === null ? "saved" : "pending");
        } else {
          // A newer edit already contains everything in `content`.
          pending ??= content;
          set("conflict");
        }
      } catch (error) {
        if (stopped) return;
        pending ??= content;
        if (retrying) again = true;
        else set("error", error);
      } finally {
        sending = null;
        retrying = false;
      }
    })();
    // `then` runs after this assignment even if `save` throws synchronously.
    inFlight = saving.then(() => {
      inFlight = null;
      if (again) {
        again = false;
        if (pending !== null && !stopped) void start();
      } else if (state === "pending" && timer === undefined) schedule();
    });
    return inFlight;
  }

  return {
    edit(content) {
      if (stopped) return;
      pending = content;
      if (state === "conflict") return;
      set("pending");
      schedule();
    },
    async flush() {
      if (stopped) return state;
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
    retry() {
      if (stopped) return;
      if (inFlight) retrying = true;
      else if (state === "error") void this.flush();
    },
    unsaved: () => pending ?? sending,
    stop() {
      stopped = true;
      clearTimeout(timer);
      timer = undefined;
      pending = null;
    },
  };
}
