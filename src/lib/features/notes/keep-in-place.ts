import type { EditorView } from "@codemirror/view";

/** The nearest ancestor that scrolls vertically, across the editor's shadow root. */
function scroller(node: Node): Element | null {
  for (let current = node.parentNode; current; ) {
    if (current instanceof ShadowRoot) {
      current = current.host;
      continue;
    }
    if (
      current instanceof Element &&
      current.scrollHeight > current.clientHeight &&
      /^(auto|scroll)$/.test(getComputedStyle(current).overflowY)
    )
      return current;
    current = current.parentNode;
  }
  return null;
}

/**
 * Once the editor has laid out its next update, scrolls so that `pos` is drawn at viewport height
 * `top` again, where the user last saw it. The editor grows with its text, so its own scroll
 * anchoring never applies: the list around it scrolls.
 */
export function keepInPlace(view: EditorView, pos: number, top: number) {
  view.requestMeasure({
    read(view) {
      const now = view.coordsAtPos(Math.min(pos, view.state.doc.length))?.top;
      return now === undefined ? null : { element: scroller(view.dom), by: now - top };
    },
    write(measured) {
      if (measured?.element && Math.abs(measured.by) >= 1)
        measured.element.scrollTop += measured.by;
    },
  });
}
