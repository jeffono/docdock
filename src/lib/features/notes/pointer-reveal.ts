// Live formatting reveals syntax around the cursor, which moves text. During a pointer gesture that
// would move text under the pointer: CodeMirror resolves the pointer again on mouseup (and on every
// move of a drag) against the new layout, so a click outside the frontmatter, which collapses it,
// landed elsewhere or became a selection. Syntax therefore follows the selection only once the
// gesture, including a double or triple click, is over.
import {
  type EditorSelection,
  type EditorState,
  StateEffect,
  StateField,
  type Transaction,
} from "@codemirror/state";
import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { keepInPlace } from "./keep-in-place";

/** The selection that reveals syntax; `held` while it ignores a pointer gesture. */
export type Reveal = { selection: EditorSelection; held: boolean };

/** Ends a hold: syntax follows the selection again. */
export const releaseReveal = StateEffect.define<null>();

/** The reveal after `transaction`: pointer selections hold it, anything else catches up. */
export function nextReveal(value: Reveal, transaction: Transaction): Reveal {
  const { changes, state } = transaction;
  if (transaction.effects.some((effect) => effect.is(releaseReveal)))
    return value.held || !value.selection.eq(state.selection)
      ? { selection: state.selection, held: false }
      : value;
  if (transaction.isUserEvent("select.pointer"))
    return value.held && changes.empty
      ? value
      : { selection: value.selection.map(changes), held: true };
  if (value.held && !transaction.selection)
    return changes.empty ? value : { selection: value.selection.map(changes), held: true };
  if (!value.held && value.selection.eq(state.selection)) return value;
  return { selection: state.selection, held: false };
}

const reveal = StateField.define<Reveal>({
  create: (state) => ({ selection: state.selection, held: false }),
  update: nextReveal,
});

/** The selection ranges that reveal syntax. */
export function revealed(state: EditorState) {
  return (state.field(reveal, false)?.selection ?? state.selection).ranges;
}

/** Whether syntax revealed by the selection may differ between the two states. */
export function revealChanged(update: Transaction | ViewUpdate) {
  return update.startState.field(reveal, false) !== update.state.field(reveal, false);
}

/**
 * How long after a mousedown another one still counts as the same double or triple click: Windows'
 * default double-click time. A shorter wait would move text between the clicks.
 */
const MULTI_CLICK_MS = 500;

export function pointerReveal() {
  return [
    reveal,
    ViewPlugin.fromClass(
      class {
        down = 0;
        timer: ReturnType<typeof setTimeout> | undefined;
        constructor(readonly view: EditorView) {
          // After CodeMirror's own mouseup listener, on the document, has placed the selection.
          view.dom.ownerDocument.defaultView?.addEventListener("mouseup", this.up);
        }
        up = () => {
          if (!this.down) return;
          clearTimeout(this.timer);
          this.timer = setTimeout(
            this.release,
            Math.max(0, this.down + MULTI_CLICK_MS - Date.now()),
          );
          this.down = 0;
        };
        release = () => {
          const { view } = this;
          if (!view.state.field(reveal).held) return;
          const head = view.state.selection.main.head;
          const top = view.coordsAtPos(head)?.top;
          view.dispatch({ effects: releaseReveal.of(null) });
          // Text above may fold or unfold: keep the clicked line where the user sees it.
          if (top !== undefined) keepInPlace(view, head, top);
        };
        destroy() {
          clearTimeout(this.timer);
          this.view.dom.ownerDocument.defaultView?.removeEventListener("mouseup", this.up);
        }
      },
      {
        eventHandlers: {
          mousedown(event) {
            if (event.button !== 0) return;
            clearTimeout(this.timer);
            this.down = Date.now();
          },
        },
      },
    ),
    // A gesture that ends without a mouseup here, as when the window loses focus, must not hold
    // syntax hidden for good.
    EditorView.domEventHandlers({
      blur(_event, view) {
        if (view.state.field(reveal).held) view.dispatch({ effects: releaseReveal.of(null) });
      },
    }),
  ];
}
