import { StateEffect, StateField } from "@codemirror/state";
import { type EditorView, showTooltip, type Tooltip, ViewPlugin } from "@codemirror/view";
import { type MessageKey, t } from "../../i18n";
import { formattingShortcuts } from "./formatting";
import { addPropertyShortcut } from "./properties";

// Lucide paths (ISC license), as in ui/Icon.svelte; Svelte styles cannot reach the shadow root.
const icons: Partial<Record<MessageKey, string>> = {
  "shortcut.bold": "M6 12h9a4 4 0 0 1 0 8H6z M6 4h8a4 4 0 0 1 0 8H6z",
  "shortcut.italic": "M19 4h-9 M14 20H5 M15 4 9 20",
  "shortcut.strike": "M16 4H9a3 3 0 0 0-2.83 4 M14 12a4 4 0 0 1 0 8H6 M4 12h16",
  "shortcut.code": "m16 18 6-6-6-6 m-8 0-6 6 6 6",
  "shortcut.link":
    "M10 13a5 5 0 0 0 7 .54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71 M14 11a5 5 0 0 0-7-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
  "shortcut.heading": "M6 4v16 M18 4v16 M6 12h12",
  "shortcut.bullet": "M8 6h13 M8 12h13 M8 18h13 M3 6h.01 M3 12h.01 M3 18h.01",
  "shortcut.task": "m3 6 1 1 2-2 M9 6h12 m-18 6 1 1 2-2 M9 12h12 m-18 6 1 1 2-2 M9 18h12",
  "shortcut.quote": "M3 21c3 0 7-4 7-11V3H3v7h7 M14 21c3 0 7-4 7-11V3h-7v7h7",
  "shortcut.property": "M4 4h16v16H4z M4 10h16 M10 4v16 M13 15h4 M15 13v4",
};

function createBar(view: EditorView) {
  const dom = document.createElement("div");
  dom.className = "formatting-bar";
  // Pointer controls preserve the selection; keyboard users keep the existing shortcuts.
  dom.addEventListener("mousedown", (event) => event.preventDefault());
  const commands = [
    ...formattingShortcuts.filter(
      (shortcut) =>
        shortcut.description in icons &&
        (shortcut.description !== "shortcut.heading" || shortcut.level === 1),
    ),
    addPropertyShortcut,
  ];
  for (const shortcut of commands) {
    const button = document.createElement("button");
    button.type = "button";
    button.tabIndex = -1;
    const key = shortcut.key
      ?.replace("Mod", "Ctrl")
      .replaceAll("-", "+")
      .replace(/\b[a-z]\b/g, (letter) => letter.toUpperCase());
    button.title = `${t(shortcut.description, { level: "level" in shortcut ? shortcut.level : 1 })} (${key})`;
    button.setAttribute("aria-label", button.title);
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("aria-hidden", "true");
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", icons[shortcut.description] as string);
    svg.append(path);
    button.append(svg);
    button.addEventListener("click", () => {
      if (view.state.readOnly || view.state.selection.ranges.length !== 1) return;
      shortcut.run?.(view);
    });
    dom.append(button);
  }
  return { dom };
}

const interaction = StateEffect.define<{ focused: boolean; dragging: boolean }>();
const interactionState = StateField.define({
  create: () => ({ focused: false, dragging: false }),
  update(value, transaction) {
    for (const effect of transaction.effects) if (effect.is(interaction)) value = effect.value;
    return value;
  },
});

const selectionTooltip = StateField.define<Tooltip | null>({
  create: () => null,
  update(value, transaction) {
    const state = transaction.state;
    const { focused, dragging } = state.field(interactionState);
    const { from, to, empty } = state.selection.main;
    if (!focused || dragging || state.readOnly || empty || state.selection.ranges.length !== 1)
      return null;
    if (value?.pos === from && value.end === to) return value;
    return { pos: from, end: to, above: true, strictSide: true, create: createBar };
  },
  provide: (field) => showTooltip.from(field),
});

export function formattingBar() {
  return [
    interactionState,
    selectionTooltip,
    ViewPlugin.fromClass(
      class {
        constructor(readonly view: EditorView) {
          view.dom.ownerDocument.addEventListener("mouseup", this.mouseup);
        }
        mouseup = () => {
          if (this.view.state.field(interactionState).dragging)
            this.view.dispatch({
              effects: interaction.of({ focused: this.view.hasFocus, dragging: false }),
            });
        };
        destroy() {
          this.view.dom.ownerDocument.removeEventListener("mouseup", this.mouseup);
        }
      },
      {
        eventHandlers: {
          mousedown(event, view) {
            if (event.button === 0)
              view.dispatch({
                effects: interaction.of({ focused: view.hasFocus, dragging: true }),
              });
          },
          focus(_event, view) {
            view.dispatch({
              effects: interaction.of({
                focused: true,
                dragging: view.state.field(interactionState).dragging,
              }),
            });
          },
          blur(_event, view) {
            view.dispatch({ effects: interaction.of({ focused: false, dragging: false }) });
          },
        },
      },
    ),
  ];
}
