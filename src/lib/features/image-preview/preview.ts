// The side image preview from the panel's side: feeds pointer, focus and key events of the note
// list to the trigger rules and runs their effects. Rust shows the window and hides it on its own.
import { imagePreviewHide, imagePreviewShow, onImagePreviewClosed } from "../../ipc";
import { HOVER_DELAY, type Input, idle, step, type Trigger } from "./trigger";

const THUMB = ".image-grid .image-thumb";

const thumbOf = (target: EventTarget | null) =>
  target instanceof Element ? target.closest<HTMLElement>(THUMB) : null;
const gridOf = (target: EventTarget | null) =>
  target instanceof Element ? target.closest(".image-grid") : null;

export type ImagePreview = ReturnType<typeof imagePreview>;

export function imagePreview() {
  let state: Trigger<HTMLElement> = idle();
  let timer = 0;
  // One call at a time, in order, so a hide never overtakes the show before it.
  let queue = Promise.resolve();
  const run = (call: () => Promise<void>) => {
    queue = queue.then(call).catch(() => {});
  };
  /** Changes on every show, hide and close, so a show still queued behind them is dropped. */
  let turn = 0;
  // A note refresh can re-render the shown or hovered thumbnail while the pointer rests: the
  // preview would then point at nothing.
  const removal = new MutationObserver(() => {
    if (state.shown && !state.shown.isConnected) send({ kind: "dismiss" });
    else if (state.waiting && !state.waiting.isConnected) send({ kind: "leave" });
  });
  let observing = false;
  function observe() {
    const active = state.shown !== null || state.waiting !== null;
    if (active === observing) return;
    observing = active;
    if (active) removal.observe(document.body, { childList: true, subtree: true });
    else removal.disconnect();
  }

  function send(input: Input<HTMLElement>) {
    const before = state;
    const [next, effect] = step(state, input);
    state = next;
    if (next.waiting !== before.waiting) {
      window.clearTimeout(timer);
      const waiting = next.waiting;
      if (waiting)
        timer = window.setTimeout(() => send({ kind: "elapsed", thumb: waiting }), HOVER_DELAY);
    }
    if (effect !== "none" || input.kind === "closed") turn++;
    observe();
    if (effect === "show" && next.shown) reveal(next.shown);
    else if (effect === "hide") run(imagePreviewHide);
  }

  function reveal(thumb: HTMLElement) {
    const image = thumb.querySelector("img");
    const path = thumb.dataset.image;
    // Rust sizes the window from the image, so only a loaded thumbnail can show.
    if (!path || !image?.complete || !image.naturalWidth || !image.naturalHeight) {
      send({ kind: "dismiss" });
      return;
    }
    const box = thumb.getBoundingClientRect();
    const rect: [number, number, number, number] = [
      Math.round(box.left),
      Math.round(box.top),
      Math.round(box.width),
      Math.round(box.height),
    ];
    const natural: [number, number] = [image.naturalWidth, image.naturalHeight];
    const mine = turn;
    run(() => {
      if (mine !== turn) return Promise.resolve();
      return imagePreviewShow(path, rect, natural).catch(() => {
        // Gone or no longer in the notes folder: hide whatever showed before it.
        if (state.shown === thumb) send({ kind: "dismiss" });
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
      if (thumb) send({ kind: "hover", thumb });
      else send(gridOf(event.target) ? { kind: "hover", thumb: null } : { kind: "leave" });
    },
    leave: () => send({ kind: "leave" }),
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
    /** Esc as its own layer, a click anywhere, any menu or dialog, and Rust's own hide. */
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
      // Popovers and dialogs: `toggle` does not bubble, but capture still sees it.
      window.addEventListener("toggle", opened, true);
      const closed = onImagePreviewClosed(() => send({ kind: "closed" }));
      return () => {
        window.removeEventListener("keydown", escapeKey, true);
        window.removeEventListener("pointerdown", dismiss, true);
        window.removeEventListener("contextmenu", dismiss, true);
        window.removeEventListener("toggle", opened, true);
        void closed.then((stop) => stop());
        dismiss();
        window.clearTimeout(timer);
        removal.disconnect();
      };
    },
  };
}
