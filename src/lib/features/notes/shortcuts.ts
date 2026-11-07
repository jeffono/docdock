import type { MessageKey } from "../../i18n";
import { formattingShortcuts } from "./formatting";

export const panelShortcuts: [string, MessageKey][] = [
  ["Ctrl+Alt+Space", "shortcut.global"],
  ["Ctrl+Tab", "shortcut.mode"],
  ["Ctrl+/", "shortcut.sheet"],
  ["↑ / ↓", "shortcut.arrows"],
  ["Alt+↑ / Alt+↓", "shortcut.direct"],
  ["Alt+Shift+↑ / Alt+Shift+↓", "shortcut.reorder"],
  ["PgUp / PgDn", "shortcut.page"],
  ["→ / ←", "shortcut.level"],
  ["Enter", "shortcut.enter"],
  ["Delete", "shortcut.delete"],
  ["Ctrl+Delete", "shortcut.deleteNote"],
  ["Ctrl+M", "shortcut.move"],
  ["Shift+F10", "shortcut.menu"],
  ["Ctrl+Enter", "shortcut.create"],
  ["Ctrl+D", "shortcut.pin"],
  ["Ctrl+E", "shortcut.edit"],
  ["Ctrl+F", "shortcut.find"],
  ["Esc", "shortcut.escape"],
];

export const editorShortcuts: { key: string; description: MessageKey; level?: number }[] = [
  ...formattingShortcuts.map(({ key, description, level }) => ({
    key: (key ?? "").replaceAll("-", "+").replace(/\b[a-z]\b/g, (key) => key.toUpperCase()),
    description,
    level,
  })),
  { key: "Enter / Backspace", description: "shortcut.continue" },
  { key: "Tab / Shift+Tab", description: "shortcut.indent" },
  { key: "Ctrl+;", description: "shortcut.property" },
  { key: "Ctrl+F / Ctrl+H", description: "shortcut.noteFind" },
  { key: "F3 / Shift+F3", description: "shortcut.noteNext" },
  { key: "Ctrl+D", description: "shortcut.occurrence" },
  { key: "Alt+click", description: "shortcut.cursor" },
];
