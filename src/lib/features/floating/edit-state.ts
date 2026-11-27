import type { AutosaveState } from "../notes/autosave";

/**
 * A post-it's inline edit, as decisions only: `step` answers the next state and the effects for
 * the post-it to run, which report back as events.
 *
 * - `read`: the note shows rendered.
 * - `entering`: asked Rust for the edit (`floatBeginEdit`), which may first move it from elsewhere.
 * - `editing`: the editor shows and saves as the user types.
 * - `leaving`: saving everything before leaving; the editor stays until the save lands.
 * - `conflict`: the note changed elsewhere; nothing saves until the user decides, and nothing
 *   leaves, so the text stays. It lasts until what they chose lands.
 * - `parked`: the notes folder went; the editor hides with its text and nothing saves until the
 *   folder returns or the user discards.
 */
export type EditMode = "read" | "entering" | "editing" | "leaving" | "conflict" | "parked";

/** What follows a leave once everything saved: reading, closing, or opening the note in the panel. */
export type After = "read" | "close" | "panel";

export type EditState = {
  mode: EditMode;
  /** While `leaving`. */
  after: After;
  /** Rust's finish request the save in `leaving` answers. */
  finish: number | null;
};

export type EditEvent =
  | { type: "begin" }
  /** Rust granted the edit; `parked` if the folder went meanwhile, which leaves nothing to show. */
  | { type: "granted"; parked: boolean }
  | { type: "refused" }
  /** Esc, Ctrl+E, the close button, "Open in panel" or a close from outside. */
  | { type: "leave"; after: After }
  /** Window focus left: save, and keep editing. */
  | { type: "blur" }
  | { type: "finish"; request: number }
  /** A flush ended in `result`. */
  | { type: "flushed"; result: AutosaveState }
  /** A save, as from the autosave's timer, found the note changed. */
  | { type: "conflict" }
  /** Rust says the note, or its path, may have changed. */
  | { type: "disk"; dirty: boolean }
  /** The note read again during a conflict: whether it still holds the version saves are based on. */
  | { type: "checked"; same: boolean }
  /** The user chose in a conflict: the text on disk, or theirs. */
  | { type: "resolve"; keep: boolean }
  /** What the user chose landed: the editor holds the note on disk, or the note theirs. */
  | { type: "resolved" }
  | { type: "parked"; dirty: boolean }
  | { type: "unparked" }
  /** The user discards the parked text. */
  | { type: "discard" };

export type Effect =
  | { type: "begin" }
  | { type: "refused" }
  | { type: "flush" }
  /** Drops the editor and draws the note again. */
  | { type: "unmount" }
  /** Ends the edit in Rust; with `handOff`, focus stays for the panel, which takes the note. */
  | { type: "end"; handOff?: true }
  | { type: "answer"; request: number; saved: boolean }
  | { type: "close" }
  | { type: "panel" }
  | { type: "render" }
  /**
   * The editor takes the note from disk; unless `force`, only if it is still clean then. `force`
   * answers `resolved` once it did.
   */
  | { type: "reload"; force: boolean }
  /** Saves the editor's text over what is on disk now; answers `resolved` once it did. */
  | { type: "keep" }
  /** Reads the note again to see whether the conflict still stands. */
  | { type: "check" }
  /** Saves what is not on disk yet, on the version saves are based on. */
  | { type: "resume" };

export const reading: EditState = { mode: "read", after: "read", finish: null };

type Step = { state: EditState; effects: Effect[] };

const stay = (state: EditState, effects: Effect[] = []): Step => ({ state, effects });
const to = (mode: EditMode, effects: Effect[] = []): Step => ({
  state: { mode, after: "read", finish: null },
  effects,
});
const answer = (request: number | null, saved: boolean): Effect[] =>
  request === null ? [] : [{ type: "answer", request, saved }];

/** What leaving does once everything saved; a finish answered `true` already ends the edit. */
function left({ after, finish }: EditState): Effect[] {
  const effects: Effect[] = [{ type: "unmount" }];
  if (finish !== null) effects.push(...answer(finish, true));
  // The window's own close ends its edit first.
  else if (after === "panel") effects.push({ type: "end", handOff: true });
  else if (after !== "close") effects.push({ type: "end" });
  if (after === "close") effects.push({ type: "close" });
  if (after === "panel") effects.push({ type: "panel" });
  return effects;
}

export function step(state: EditState, event: EditEvent): Step {
  const { mode } = state;
  switch (event.type) {
    case "begin":
      return mode === "read" ? to("entering", [{ type: "begin" }]) : stay(state);
    case "granted":
      if (mode !== "entering") return stay(state);
      // Nothing was shown nor typed: the edit ends, and the window with its folder.
      return event.parked ? to("read", [{ type: "end" }]) : to("editing");
    case "refused":
      return mode === "entering" ? to("read", [{ type: "refused" }]) : stay(state);
    case "leave":
      if (mode === "read") {
        return stay(state, event.after === "read" ? [] : [{ type: event.after }]);
      }
      if (mode === "editing") {
        return {
          state: { mode: "leaving", after: event.after, finish: null },
          effects: [{ type: "flush" }],
        };
      }
      if (mode === "leaving") {
        // A close always stands; the panel replaces only a plain leave, and not once Rust asked
        // to finish, which the panel must not undo by taking the note back.
        const replaces =
          event.after === "close" ||
          (event.after === "panel" && state.after === "read" && state.finish === null);
        return replaces ? stay({ ...state, after: event.after }) : stay(state);
      }
      // A conflict or a parked edit keeps its text: only the user's choice leaves.
      return stay(state);
    case "blur":
      return mode === "editing" ? stay(state, [{ type: "flush" }]) : stay(state);
    case "finish":
      if (mode === "editing") {
        return {
          state: { mode: "leaving", after: "read", finish: event.request },
          effects: [{ type: "flush" }],
        };
      }
      if (mode === "leaving") {
        // Rust ignores the earlier request's answer once it asks again. Whoever asked takes the
        // note, or exits: the panel no longer does.
        const after = state.after === "panel" ? "read" : state.after;
        return stay({ ...state, after, finish: event.request });
      }
      // Nothing to save while reading; anything else could not save now.
      return stay(state, answer(event.request, mode === "read"));
    case "flushed":
      if (mode === "leaving") {
        if (event.result === "saved") return to("read", left(state));
        return to(
          event.result === "conflict" ? "conflict" : "editing",
          answer(state.finish, false),
        );
      }
      if (mode === "editing" && event.result === "conflict") return to("conflict");
      return stay(state);
    case "conflict":
      // A flush under way reports it as its result.
      return mode === "editing" ? to("conflict") : stay(state);
    case "disk":
      if (mode === "read") return stay(state, [{ type: "render" }]);
      if (mode === "editing") {
        // A disk change never replaces text not yet saved.
        return stay(state, event.dirty ? [] : [{ type: "reload", force: false }]);
      }
      if (mode === "conflict") return stay(state, [{ type: "check" }]);
      return stay(state);
    case "checked":
      return mode === "conflict" && event.same ? to("editing", [{ type: "resume" }]) : stay(state);
    case "resolve":
      // Still a conflict until the choice lands: one that fails can be chosen again.
      if (mode !== "conflict") return stay(state);
      return stay(state, [event.keep ? { type: "keep" } : { type: "reload", force: true }]);
    case "resolved":
      return mode === "conflict" ? to("editing") : stay(state);
    case "parked":
      if (mode !== "editing" && mode !== "leaving" && mode !== "conflict") return stay(state);
      // Nothing to keep: the edit ends, and the window with it.
      if (!event.dirty)
        return to("read", [...answer(state.finish, false), { type: "unmount" }, { type: "end" }]);
      return to("parked", answer(state.finish, false));
    case "unparked":
      return mode === "parked" ? to("editing", [{ type: "resume" }]) : stay(state);
    case "discard":
      return mode === "parked" ? to("read", [{ type: "unmount" }, { type: "end" }]) : stay(state);
  }
}
