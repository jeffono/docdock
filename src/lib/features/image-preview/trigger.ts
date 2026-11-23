// When the side preview shows, switches and hides: pure rules over image thumbnails and unpinned
// closed notes (`T`), whose effects `preview.ts` runs.
import type { NotePreviewMode } from "../../ipc";

export type Target<T> = { kind: "image" | "note"; item: T };
export type DelayKind = "hover" | "noteSwitch" | "noteFocus";

/** The target shown, waiting for a delay, and hovered/selected note tracking. */
export type Trigger<T> = {
  shown: Target<T> | null;
  waiting: (Target<T> & { delay: DelayKind }) | null;
  hoveredNote: T | null;
  selectedNote: T | null;
  noteMode: NotePreviewMode;
  altHeld: boolean;
  dragging: boolean;
};

export type Input<T> =
  /** The pointer is over a thumbnail of a grid, or over the grid between them (`null`). */
  | { kind: "hover"; thumb: T | null }
  /** The pointer is over an unpinned closed note (`note`), or outside one (`null`). */
  | { kind: "hoverNote"; note: T | null }
  /** Keyboard selection in the list moved to an unpinned closed note (`note`), or elsewhere (`null`). */
  | { kind: "selectNote"; note: T | null }
  /** The pointer left the grid or list. */
  | { kind: "leave" }
  /** The delay of `target` ended. */
  | { kind: "elapsed"; target: Target<T> }
  /** Keyboard focus reached a thumbnail. */
  | { kind: "focus"; thumb: T }
  /** Space on a focused thumbnail. */
  | { kind: "toggle"; thumb: T }
  /** Alt key state changed (outside AltGr); `selected` resolves focused note when Alt turns on. */
  | { kind: "alt"; held: boolean; selected?: T | null }
  /** Note preview preference changed. */
  | { kind: "noteMode"; mode: NotePreviewMode }
  /** A card/pin drag started or ended. */
  | { kind: "dragging"; active: boolean }
  /** Esc, a click, scrolling, editing, another mode, search typing or a menu. */
  | { kind: "dismiss" }
  /** Rust already hid it, as when the panel hides. */
  | { kind: "closed" };

/** What the caller must do; the timer follows `waiting` alone. */
export type Effect = "none" | "show" | "hide";

/** Delay before hovering a thumbnail or closed note shows it, once nothing is shown yet. */
export const HOVER_DELAY = 300;
/** Short debounce when sweeping between closed notes so rapid moves do not flood disk reads. */
export const NOTE_SWITCH_DELAY = 60;
/** Delay when moving keyboard selection onto a closed note in hover/focus mode. */
export const NOTE_FOCUS_DELAY = 150;

export function idle<T>(noteMode: NotePreviewMode = "hover"): Trigger<T> {
  return {
    shown: null,
    waiting: null,
    hoveredNote: null,
    selectedNote: null,
    noteMode,
    altHeld: false,
    dragging: false,
  };
}

function clearView<T>(state: Trigger<T>, patch: Partial<Trigger<T>> = {}): [Trigger<T>, Effect] {
  return [
    { ...state, ...patch, shown: null, waiting: null },
    state.shown === null ? "none" : "hide",
  ];
}

function showTarget<T>(
  state: Trigger<T>,
  target: Target<T>,
  patch: Partial<Trigger<T>> = {},
): [Trigger<T>, Effect] {
  const same = state.shown?.kind === target.kind && state.shown.item === target.item;
  return [{ ...state, ...patch, shown: target, waiting: null }, same ? "none" : "show"];
}

function sameTarget<T>(a: Target<T> | null, b: Target<T> | null): boolean {
  return a !== null && b !== null && a.kind === b.kind && a.item === b.item;
}

export function step<T>(state: Trigger<T>, input: Input<T>): [Trigger<T>, Effect] {
  switch (input.kind) {
    case "closed":
      return [
        { ...state, shown: null, waiting: null, hoveredNote: null, selectedNote: null },
        "none",
      ];
    case "dismiss":
      return clearView(state, { hoveredNote: null, selectedNote: null });
    case "leave":
      return clearView(state, { hoveredNote: null });
    case "dragging":
      if (input.active) return clearView(state, { hoveredNote: null, dragging: true });
      return [{ ...state, dragging: false }, "none"];
    case "noteMode": {
      if (input.mode === state.noteMode) return [state, "none"];
      const altHeld = input.mode === "alt" ? state.altHeld : false;
      const base: Trigger<T> = { ...state, noteMode: input.mode, altHeld };
      if (state.shown?.kind === "note") return clearView(base);
      if (state.waiting?.kind === "note") return [{ ...base, waiting: null }, "none"];
      return [base, "none"];
    }
    case "alt": {
      const selected = input.selected !== undefined ? input.selected : state.selectedNote;
      if (input.held === state.altHeld && selected === state.selectedNote) return [state, "none"];
      const base: Trigger<T> = { ...state, altHeld: input.held, selectedNote: selected };
      if (state.noteMode !== "alt" || state.dragging) return [base, "none"];
      if (!input.held) {
        if (state.shown?.kind === "note") return clearView(base);
        if (state.waiting?.kind === "note") return [{ ...base, waiting: null }, "none"];
        return [base, "none"];
      }
      if (state.shown?.kind === "image") return [base, "none"];
      const candidate = state.hoveredNote ?? selected;
      return candidate !== null
        ? showTarget(base, { kind: "note", item: candidate })
        : [base, "none"];
    }
    case "hover": {
      if (state.dragging) return [state, "none"];
      if (input.thumb === null) {
        if (state.shown?.kind === "note") return clearView(state, { hoveredNote: null });
        return [
          {
            ...state,
            hoveredNote: null,
            waiting: state.waiting?.kind === "image" ? null : state.waiting,
          },
          "none",
        ];
      }
      const target: Target<T> = { kind: "image", item: input.thumb };
      // Once an image is shown, moving to another thumbnail switches at once.
      if (state.shown?.kind === "image") return showTarget(state, target, { hoveredNote: null });
      if (state.shown?.kind === "note")
        return [
          { ...state, shown: null, hoveredNote: null, waiting: { ...target, delay: "hover" } },
          "hide",
        ];
      return [{ ...state, hoveredNote: null, waiting: { ...target, delay: "hover" } }, "none"];
    }
    case "hoverNote": {
      if (state.dragging) return [state, "none"];
      const base: Trigger<T> = { ...state, hoveredNote: input.note };
      if (state.shown?.kind === "image") {
        if (
          input.note === null ||
          state.noteMode === "off" ||
          (state.noteMode === "alt" && !state.altHeld)
        )
          return clearView(base);
        if (state.noteMode === "alt") return showTarget(base, { kind: "note", item: input.note });
        return [
          { ...base, shown: null, waiting: { kind: "note", item: input.note, delay: "hover" } },
          "hide",
        ];
      }
      if (state.noteMode === "off") return [base, "none"];
      if (state.noteMode === "alt") {
        if (!state.altHeld) return [base, "none"];
        if (input.note === null)
          return state.shown?.kind === "note" || state.waiting?.kind === "note"
            ? clearView(base)
            : [base, "none"];
        if (state.shown?.kind === "note") {
          if (state.shown.item === input.note) return [{ ...base, waiting: null }, "none"];
          return [
            {
              ...base,
              waiting: { kind: "note", item: input.note, delay: "noteSwitch" },
            },
            "none",
          ];
        }
        return showTarget(base, { kind: "note", item: input.note });
      }
      if (input.note === null) {
        return state.shown?.kind === "note" || state.waiting?.kind === "note"
          ? clearView(base)
          : [base, "none"];
      }
      if (state.shown?.kind === "note") {
        if (state.shown.item === input.note) return [{ ...base, waiting: null }, "none"];
        return [
          { ...base, waiting: { kind: "note", item: input.note, delay: "noteSwitch" } },
          "none",
        ];
      }
      return [{ ...base, waiting: { kind: "note", item: input.note, delay: "hover" } }, "none"];
    }
    case "selectNote": {
      const base: Trigger<T> = {
        ...state,
        selectedNote: input.note,
        hoveredNote: input.note !== null ? null : state.hoveredNote,
      };
      if (state.dragging || state.shown?.kind === "image" || state.noteMode === "off")
        return [base, "none"];
      if (input.note === null) {
        if (
          state.hoveredNote === null &&
          (state.shown?.kind === "note" || state.waiting?.kind === "note")
        )
          return clearView(base);
        return [base, "none"];
      }
      if (state.noteMode === "alt") {
        if (!state.altHeld) return [base, "none"];
        if (state.shown?.kind === "note") {
          if (state.shown.item === input.note) return [{ ...base, waiting: null }, "none"];
          return [
            { ...base, waiting: { kind: "note", item: input.note, delay: "noteSwitch" } },
            "none",
          ];
        }
        return showTarget(base, { kind: "note", item: input.note });
      }
      if (state.shown?.kind === "note") {
        if (state.shown.item === input.note) return [{ ...base, waiting: null }, "none"];
        return [
          { ...base, waiting: { kind: "note", item: input.note, delay: "noteSwitch" } },
          "none",
        ];
      }
      return [{ ...base, waiting: { kind: "note", item: input.note, delay: "noteFocus" } }, "none"];
    }
    case "elapsed": {
      if (state.dragging || !sameTarget(state.waiting, input.target)) return [state, "none"];
      if (
        input.target.kind === "note" &&
        (state.noteMode === "off" || (state.noteMode === "alt" && !state.altHeld))
      )
        return [{ ...state, waiting: null }, "none"];
      return showTarget(state, input.target);
    }
    case "focus":
      if (state.dragging) return [state, "none"];
      return state.shown?.kind === "image" && state.shown.item !== input.thumb
        ? showTarget(state, { kind: "image", item: input.thumb })
        : [state, "none"];
    case "toggle": {
      if (state.dragging) return [state, "none"];
      const target: Target<T> = { kind: "image", item: input.thumb };
      return sameTarget(state.shown, target) ? clearView(state) : showTarget(state, target);
    }
  }
}
