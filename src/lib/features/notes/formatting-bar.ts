import { Prec, StateEffect, StateField } from "@codemirror/state";
import { EditorView, showTooltip, type Tooltip, ViewPlugin } from "@codemirror/view";
import { type MessageKey, t } from "../../i18n";
import { formattingShortcuts } from "./formatting";
import { addPropertyShortcut } from "./properties";
import { textTransforms } from "./transforms";

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
  "transform.menu": "m3 15 4-8 4 8 M4 13h6 M20 15v-4m0 2a2 2 0 1 0-4 0 2 2 0 0 0 4 0",
};

function iconButton(label: string, icon: MessageKey) {
  const button = document.createElement("button");
  button.type = "button";
  button.tabIndex = -1;
  button.title = label;
  button.setAttribute("aria-label", label);
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("d", icons[icon] ?? "");
  svg.append(path);
  button.append(svg);
  return button;
}

const openMenus = new WeakMap<EditorView, (event: KeyboardEvent) => boolean>();
let menuNumber = 0;

function addTransforms(dom: HTMLElement, view: EditorView) {
  const label = t("transform.menu");
  const trigger = iconButton(label, "transform.menu");
  const menu = document.createElement("div");
  menu.className = "transform-menu";
  menu.id = `note-transforms-${++menuNumber}`;
  menu.hidden = true;
  menu.setAttribute("role", "menu");
  menu.setAttribute("aria-label", label);
  trigger.setAttribute("aria-haspopup", "menu");
  trigger.setAttribute("aria-expanded", "false");
  trigger.setAttribute("aria-controls", menu.id);
  dom.append(trigger, menu);

  let active = 0;
  const activate = (index: number) => {
    items[active].removeAttribute("data-active");
    active = index;
    items[active].setAttribute("data-active", "");
    view.contentDOM.setAttribute("aria-activedescendant", items[active].id);
    const { offsetTop, offsetHeight } = items[active];
    if (offsetTop < menu.scrollTop) menu.scrollTop = offsetTop;
    if (offsetTop + offsetHeight > menu.scrollTop + menu.clientHeight)
      menu.scrollTop = offsetTop + offsetHeight - menu.clientHeight;
  };
  const close = () => {
    if (menu.hidden) return;
    menu.hidden = true;
    trigger.setAttribute("aria-expanded", "false");
    view.contentDOM.removeAttribute("aria-activedescendant");
    view.contentDOM.removeAttribute("aria-controls");
    openMenus.delete(view);
  };
  const items = textTransforms.map((command, index) => {
    if (index > 0 && textTransforms[index - 1].group !== command.group) {
      const separator = document.createElement("hr");
      separator.setAttribute("role", "separator");
      menu.append(separator);
    }
    const item = document.createElement("button");
    item.type = "button";
    item.tabIndex = -1;
    item.id = `${menu.id}-${command.id}`;
    item.setAttribute("role", "menuitem");
    item.textContent = t(command.label);
    item.onclick = () => {
      close();
      if (view.state.selection.ranges.length === 1) command.run(view);
    };
    menu.append(item);
    return item;
  });

  trigger.onclick = () => {
    if (!menu.hidden) {
      close();
      return;
    }
    if (view.state.readOnly || view.state.selection.ranges.length !== 1) return;
    menu.hidden = false;
    trigger.setAttribute("aria-expanded", "true");
    view.contentDOM.setAttribute("aria-controls", menu.id);
    const bar = dom.getBoundingClientRect();
    const below = innerHeight - bar.bottom;
    const above = below < menu.scrollHeight && bar.top > below;
    menu.classList.toggle("above", above);
    menu.style.setProperty("--transform-menu-room", `${above ? bar.top : below}px`);
    activate(0);
    openMenus.set(view, (event) => {
      switch (event.key) {
        case "ArrowDown":
          activate((active + 1) % items.length);
          break;
        case "ArrowUp":
          activate((active + items.length - 1) % items.length);
          break;
        case "Home":
          activate(0);
          break;
        case "End":
          activate(items.length - 1);
          break;
        case "Enter":
        case " ":
          items[active].click();
          break;
        case "Escape":
          close();
          break;
        default:
          close();
          return false;
      }
      event.stopPropagation();
      return true;
    });
  };
  return close;
}

function createBar(view: EditorView) {
  const dom = document.createElement("div");
  dom.className = "formatting-bar";
  // Keep editor focus and selection while using pointer controls.
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
    const key = shortcut.key
      ?.replace("Mod", "Ctrl")
      .replaceAll("-", "+")
      .replace(/\b[a-z]\b/g, (letter) => letter.toUpperCase());
    const label = t(shortcut.description, { level: "level" in shortcut ? shortcut.level : 1 });
    const button = iconButton(`${label} (${key})`, shortcut.description);
    button.addEventListener("click", () => {
      closeMenu();
      if (view.state.readOnly || view.state.selection.ranges.length !== 1) return;
      shortcut.run?.(view);
    });
    dom.append(button);
  }
  const closeMenu = addTransforms(dom, view);
  return { dom, destroy: closeMenu };
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
    // The open menu handles Esc before completion, find, or leaving the editor.
    Prec.highest(
      EditorView.domEventHandlers({
        keydown: (event, view) => openMenus.get(view)?.(event) ?? false,
      }),
    ),
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
