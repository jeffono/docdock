// When the side image preview shows, switches and hides: pure rules over thumbnails (`T`), whose
// effects `preview.ts` runs. Behavior in "Image preview" in `docs/DESIGN.md`.

/** The thumbnail shown, and the one hovered that waits for the delay. */
export type Trigger<T> = { shown: T | null; waiting: T | null };

export type Input<T> =
  /** The pointer is over a thumbnail of a grid, or over the grid between them (`null`). */
  | { kind: "hover"; thumb: T | null }
  /** The pointer left the grid. */
  | { kind: "leave" }
  /** The hover delay of `thumb` ended. */
  | { kind: "elapsed"; thumb: T }
  /** Keyboard focus reached a thumbnail. */
  | { kind: "focus"; thumb: T }
  /** Space on a focused thumbnail. */
  | { kind: "toggle"; thumb: T }
  /** Esc, a click, scrolling, editing, another mode or a menu. */
  | { kind: "dismiss" }
  /** Rust already hid it, as when the panel hides. */
  | { kind: "closed" };

/** What the caller must do; the hover timer follows `waiting` alone. */
export type Effect = "none" | "show" | "hide";

/** Delay before hovering a thumbnail shows it, once nothing is shown yet. */
export const HOVER_DELAY = 300;

export function idle<T>(): Trigger<T> {
  return { shown: null, waiting: null };
}

export function step<T>(state: Trigger<T>, input: Input<T>): [Trigger<T>, Effect] {
  const hidden: [Trigger<T>, Effect] = [idle(), state.shown === null ? "none" : "hide"];
  switch (input.kind) {
    case "hover":
      if (input.thumb === null) return [{ ...state, waiting: null }, "none"];
      // Once shown, moving to another thumbnail switches at once.
      if (state.shown !== null)
        return state.shown === input.thumb
          ? [state, "none"]
          : [{ shown: input.thumb, waiting: null }, "show"];
      return [{ shown: null, waiting: input.thumb }, "none"];
    case "elapsed":
      return state.waiting === input.thumb
        ? [{ shown: input.thumb, waiting: null }, "show"]
        : [state, "none"];
    case "focus":
      return state.shown !== null && state.shown !== input.thumb
        ? [{ shown: input.thumb, waiting: null }, "show"]
        : [state, "none"];
    case "toggle":
      return state.shown === input.thumb ? hidden : [{ shown: input.thumb, waiting: null }, "show"];
    case "leave":
    case "dismiss":
      return hidden;
    case "closed":
      return [idle(), "none"];
  }
}
