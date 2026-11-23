// The side preview from the panel's side: feeds pointer, focus, key and Alt events of the note
// list to the trigger rules and runs their effects. Rust shows the window and hides it on its own.
import {
  imagePreviewHide,
  imagePreviewShow,
  imagePreviewShowNote,
  type NotePreviewMode,
  onImagePreviewClosed,
  onPanelAlt,
} from "../../ipc";
import {
  type DelayKind,
  HOVER_DELAY,
  type Input,
  idle,
  NOTE_FOCUS_DELAY,
  NOTE_SWITCH_DELAY,
  step,
  type Target,
  type Trigger,
} from "./trigger";

const THUMB = ".image-grid .image-thumb";
const NOTE_LINE = '.card.line[data-note][data-path]:not([aria-busy="true"])';

const thumbOf = (target: EventTarget | null) =>
  target instanceof Element ? target.closest<HTMLElement>(THUMB) : null;
const gridOf = (target: EventTarget | null) =>
  target instanceof Element ? target.closest(".image-grid") : null;
const noteLineOf = (target: EventTarget | null) =>
  target instanceof Element ? target.closest<HTMLElement>(NOTE_LINE) : null;

function validTarget(target: Target<HTMLElement>): boolean {
  if (!target.item.isConnected) return false;
  return target.kind === "image" ? target.item.matches(THUMB) : target.item.matches(NOTE_LINE);
}

function sameWaiting(
  a: (Target<HTMLElement> & { delay: DelayKind }) | null,
  b: (Target<HTMLElement> & { delay: DelayKind }) | null,
): boolean {
  if (a === b) return true;
  return a !== null && b !== null && a.kind === b.kind && a.item === b.item && a.delay === b.delay;
}

function delayMs(delay: DelayKind): number {
  switch (delay) {
    case "hover":
      return HOVER_DELAY;
    case "noteSwitch":
      return NOTE_SWITCH_DELAY;
    case "noteFocus":
      return NOTE_FOCUS_DELAY;
  }
}

export type ImagePreview = ReturnType<typeof imagePreview>;

export function imagePreview(selectedNote?: () => HTMLElement | null) {
  let state: Trigger<HTMLElement> = idle();
  let timer = 0;
  // One call at a time, in order, so a hide never overtakes the show before it.
  let queue = Promise.resolve();
  const run = (call: () => Promise<void>) => {
    queue = queue.then(call).catch(() => {});
  };
  /** Changes on every show, hide and close, so a show still queued behind them is dropped. */
  let turn = 0;
  // A list refresh, toggle, pin or delete can change or detach the active item while hovered.
  const mutation = new MutationObserver(() => {
    if (state.shown && !validTarget(state.shown)) {
      send({ kind: "dismiss" });
      return;
    }
    if (state.waiting && !validTarget(state.waiting)) {
      if (state.waiting.kind === "note") {
        if (state.hoveredNote === state.waiting.item) send({ kind: "hoverNote", note: null });
        else if (state.selectedNote === state.waiting.item)
          send({ kind: "selectNote", note: null });
        else send({ kind: "dismiss" });
      } else {
        send({ kind: "leave" });
      }
      return;
    }
    if (
      state.hoveredNote &&
      (!state.hoveredNote.isConnected || !state.hoveredNote.matches(NOTE_LINE))
    ) {
      send({ kind: "hoverNote", note: null });
    }
  });
  let observing = false;
  function observe() {
    const active = state.shown !== null || state.waiting !== null || state.hoveredNote !== null;
    if (active === observing) return;
    observing = active;
    if (active) {
      mutation.observe(document.body, {
        childList: true,
        subtree: true,
        attributes: true,
        attributeFilter: ["class", "aria-busy"],
      });
    } else {
      mutation.disconnect();
    }
  }

  function send(input: Input<HTMLElement>) {
    const before = state;
    const [next, effect] = step(state, input);
    state = next;
    if (!sameWaiting(before.waiting, next.waiting)) {
      window.clearTimeout(timer);
      const waiting = next.waiting;
      if (waiting) {
        const target: Target<HTMLElement> = { kind: waiting.kind, item: waiting.item };
        timer = window.setTimeout(() => send({ kind: "elapsed", target }), delayMs(waiting.delay));
      }
    }
    if (effect !== "none" || input.kind === "closed") turn++;
    observe();
    if (effect === "show" && next.shown) reveal(next.shown);
    else if (effect === "hide") run(imagePreviewHide);
  }

  function reveal(target: Target<HTMLElement>) {
    const box = target.item.getBoundingClientRect();
    const rect: [number, number, number, number] = [
      Math.round(box.left),
      Math.round(box.top),
      Math.round(box.width),
      Math.round(box.height),
    ];
    if (target.kind === "image") {
      const image = target.item.querySelector("img");
      const path = target.item.dataset.image;
      // Rust sizes the window from the image, so only a loaded thumbnail can show.
      if (!path || !image?.complete || !image.naturalWidth || !image.naturalHeight) {
        send({ kind: "dismiss" });
        return;
      }
      const natural: [number, number] = [image.naturalWidth, image.naturalHeight];
      const mine = turn;
      run(() => {
        if (mine !== turn) return Promise.resolve();
        return imagePreviewShow(path, rect, natural).catch(() => {
          // Gone or no longer in the notes folder: hide whatever showed before it.
          if (state.shown?.kind === "image" && state.shown.item === target.item) {
            send({ kind: "dismiss" });
          }
        });
      });
      return;
    }
    const path = target.item.dataset.path;
    if (!path) {
      send({ kind: "dismiss" });
      return;
    }
    const mine = turn;
    run(() => {
      if (mine !== turn) return Promise.resolve();
      return imagePreviewShowNote(path, rect).catch(() => {
        if (state.shown?.kind === "note" && state.shown.item === target.item) {
          send({ kind: "dismiss" });
        }
      });
    });
  }

  const dismiss = () => send({ kind: "dismiss" });

  return {
    dismiss,
    /** Rust hid it already, as when the panel hides. */
    closed: () => send({ kind: "closed" }),
    pointerover(event: PointerEvent) {
      const thumb = thumbOf(event.target);
      if (thumb) {
        send({ kind: "hover", thumb });
        return;
      }
      if (gridOf(event.target)) {
        send({ kind: "hover", thumb: null });
        return;
      }
      const note = noteLineOf(event.target);
      if (note) {
        send({ kind: "hoverNote", note });
        return;
      }
      send({ kind: "leave" });
    },
    leave: () => send({ kind: "leave" }),
    selectNote(note: HTMLElement | null) {
      send({ kind: "selectNote", note: note?.matches(NOTE_LINE) ? note : null });
    },
    setNoteMode(mode: NotePreviewMode) {
      send({ kind: "noteMode", mode });
    },
    setDragging(active: boolean) {
      send({ kind: "dragging", active });
    },
    focusin(event: FocusEvent) {
      const thumb = thumbOf(event.target);
      if (thumb) send({ kind: "focus", thumb });
    },
    focusout(event: FocusEvent) {
      const grid = gridOf(event.target);
      if (grid && !(event.relatedTarget instanceof Node && grid.contains(event.relatedTarget)))
        dismiss();
    },
    /** Space on a focused thumbnail toggles it, instead of pressing the button. */
    keydown(event: KeyboardEvent) {
      const thumb = thumbOf(event.target);
      if (!thumb || event.key !== " " || event.ctrlKey || event.altKey || event.metaKey) return;
      event.preventDefault();
      if (!event.repeat) send({ kind: "toggle", thumb });
    },
    /** Esc as its own layer, a click anywhere, drags, menus, Alt state and Rust's own hide. */
    attach() {
      const escapeKey = (event: KeyboardEvent) => {
        if (event.key !== "Escape" || event.isComposing || state.shown === null) return;
        event.preventDefault();
        event.stopImmediatePropagation();
        dismiss();
      };
      const opened = (event: Event) => {
        if (event instanceof ToggleEvent && event.newState === "open") dismiss();
      };
      window.addEventListener("keydown", escapeKey, true);
      window.addEventListener("pointerdown", dismiss, true);
      window.addEventListener("contextmenu", dismiss, true);
      window.addEventListener("dragstart", dismiss, true);
      // Popovers and dialogs: `toggle` does not bubble, but capture still sees it.
      window.addEventListener("toggle", opened, true);
      const closed = onImagePreviewClosed(() => send({ kind: "closed" }));
      const panelAlt = onPanelAlt((held) => {
        if (!held) {
          send({ kind: "alt", held: false });
          return;
        }
        const current = selectedNote?.() ?? null;
        send({ kind: "alt", held: true, selected: current?.matches(NOTE_LINE) ? current : null });
      });
      return () => {
        window.removeEventListener("keydown", escapeKey, true);
        window.removeEventListener("pointerdown", dismiss, true);
        window.removeEventListener("contextmenu", dismiss, true);
        window.removeEventListener("dragstart", dismiss, true);
        window.removeEventListener("toggle", opened, true);
        void closed.then((stop) => stop());
        void panelAlt.then((stop) => stop());
        dismiss();
        window.clearTimeout(timer);
        mutation.disconnect();
      };
    },
  };
}
