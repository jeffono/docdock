import { isolateHistory } from "@codemirror/commands";
import { EditorSelection, type EditorState } from "@codemirror/state";
import type { Command, KeyBinding } from "@codemirror/view";
import type { MessageKey } from "../../i18n";
import type { EditorCommand } from "./command";

/** Only remove complete matching pairs; never consume adjacent user text. */
export function toggleInline(text: string, from: number, to: number, marker: string) {
  const size = marker.length;
  if (
    to - from >= size * 2 &&
    text.slice(from, from + size) === marker &&
    text.slice(to - size, to) === marker
  )
    return {
      changes: [
        { from, to: from + size },
        { from: to - size, to },
      ],
      anchor: from,
      head: to - size * 2,
    };
  if (
    from >= size &&
    text.slice(from - size, from) === marker &&
    text.slice(to, to + size) === marker
  )
    return {
      changes: [
        { from: from - size, to: from },
        { from: to, to: to + size },
      ],
      anchor: from - size,
      head: to - size,
    };
  return {
    changes: [
      { from, insert: marker },
      { from: to, insert: marker },
    ],
    anchor: from + size,
    head: to + size,
  };
}

function inline(marker: string): Command {
  return (view) => {
    if (view.state.readOnly) return false;
    const { from, to, anchor } = view.state.selection.main;
    const edit = toggleInline(view.state.doc.toString(), from, to, marker);
    view.dispatch({
      changes: edit.changes,
      selection:
        anchor === from
          ? { anchor: edit.anchor, head: edit.head }
          : { anchor: edit.head, head: edit.anchor },
      annotations: isolateHistory.of("full"),
      userEvent: "input",
    });
    return true;
  };
}

/** Replace only the joined lines; positions are normalized CodeMirror offsets. */
export function joinLines(text: string, from: number, originalTo: number) {
  // A selection ending at the start of a line does not include that line, as in `block()`.
  const to = originalTo > from && text[originalTo - 1] === "\n" ? originalTo - 1 : originalTo;
  const start = text.slice(0, from).lastIndexOf("\n") + 1;
  let end = text.indexOf("\n", to);
  if (end < 0) end = text.length;
  // A caret (or selection within one line) joins the following line too.
  if (!text.slice(from, to).includes("\n") && end < text.length) {
    const next = text.indexOf("\n", end + 1);
    end = next < 0 ? text.length : next;
  }
  const lines = text.slice(start, end).split("\n");
  const insert = lines.reduce((joined, line) => {
    const content = line.replace(
      /^\s*(?:(?:[-+*]|\d+[.)])\s+(?:\[[ xX]\]\s+)?|\[[ xX]\]\s+|>\s*)?/,
      "",
    );
    return joined + (!joined || /\s$/.test(joined) ? "" : " ") + content;
  });
  return { from: start, to: end, insert };
}

const join: Command = (view) => {
  if (view.state.readOnly) return false;
  const { from, to } = view.state.selection.main;
  const change = joinLines(view.state.doc.toString(), from, to);
  view.dispatch({
    changes: change,
    selection: EditorSelection.cursor(change.from + change.insert.length),
    annotations: isolateHistory.of("full"),
    userEvent: "input",
  });
  return true;
};

type Block = "bullet" | "number" | "task" | "quote" | number;
function block(kind: Block): EditorCommand {
  return (view, edit = view) => {
    const { state } = edit;
    if (state.readOnly) return false;
    const { from, to } = state.selection.main;
    const first = state.doc.lineAt(from).number;
    const lastLine = state.doc.lineAt(to);
    const last = to > from && to === lastLine.from ? lastLine.number - 1 : lastLine.number;
    const lines = Array.from({ length: last - first + 1 }, (_, i) => state.doc.line(first + i));
    const pattern =
      typeof kind === "number"
        ? /^(\s{0,3})(#{1,6}(?:\s+|$))?/
        : kind === "quote"
          ? /^(\s*)(>\s?)?/
          : /^(\s*)((?:[-+*]|\d+[.)])\s+(?:\[[ xX]\]\s+)?)?/;
    const matches = lines.map((line) => line.text.match(pattern) as RegExpMatchArray);
    const same = matches.every((m) =>
      typeof kind === "number"
        ? kind > 0 && m[2]?.trim() === "#".repeat(kind)
        : kind === "quote"
          ? !!m[2]
          : kind === "task"
            ? /\[[ xX]\]/.test(m[2] ?? "")
            : kind === "number"
              ? /^\d+[.)]\s+(?!\[[ xX]\])/.test(m[2] ?? "")
              : kind === "bullet"
                ? /^[-+*]\s+(?!\[[ xX]\])/.test(m[2] ?? "")
                : /^\d+[.)]\s+(?!\[[ xX]\])/.test(m[2] ?? ""),
    );
    const changes = lines
      .map((line, i) => {
        const match = matches[i];
        const prefix = same
          ? ""
          : typeof kind === "number"
            ? kind
              ? `${"#".repeat(kind)} `
              : ""
            : kind === "quote"
              ? "> "
              : kind === "task"
                ? "- [ ] "
                : kind === "bullet"
                  ? "- "
                  : `${i + 1}. `;
        return {
          from: line.from + match[1].length,
          to: line.from + match[0].length,
          insert: prefix,
        };
      })
      .filter((c) => state.sliceDoc(c.from, c.to) !== c.insert);
    edit.dispatch({ changes, annotations: isolateHistory.of("full"), userEvent: "input" });
    return true;
  };
}

const link: EditorCommand = (view, edit = view) => {
  if (edit.state.readOnly) return false;
  const { from, to } = edit.state.selection.main;
  const text = edit.state.sliceDoc(from, to);
  edit.dispatch({
    changes: { from, to, insert: `[${text}]()` },
    selection: EditorSelection.cursor(from + (text ? text.length + 3 : 1)),
    annotations: isolateHistory.of("full"),
    userEvent: "input",
  });
  return true;
};

export function inList(state: EditorState) {
  return /^\s*(?:[-+*]|\d+[.)])\s/.test(state.doc.lineAt(state.selection.main.head).text);
}

export const formattingShortcuts: (KeyBinding & {
  description: MessageKey;
  level?: number;
  run: EditorCommand;
  slash?: { id: string; label: MessageKey };
})[] = [
  { description: "shortcut.join", key: "Ctrl-j", run: join, preventDefault: true },
  { description: "shortcut.bold", key: "Ctrl-b", run: inline("**") },
  { description: "shortcut.italic", key: "Ctrl-i", run: inline("*") },
  { description: "shortcut.strike", key: "Ctrl-Shift-x", run: inline("~~") },
  { description: "shortcut.code", key: "Ctrl-Shift-`", run: inline("`"), preventDefault: true },
  {
    description: "shortcut.link",
    key: "Ctrl-k",
    run: link,
    slash: { id: "link", label: "slash.link" },
  },
  ...Array.from({ length: 7 }, (_, level) => ({
    description: (level ? "shortcut.heading" : "shortcut.paragraph") as MessageKey,
    level,
    key: `Ctrl-${level}`,
    run: block(level),
    slash:
      level >= 1 && level <= 3
        ? { id: `heading-${level}`, label: "slash.heading" as const }
        : undefined,
  })),
  {
    description: "shortcut.bullet",
    key: "Ctrl-Shift-8",
    run: block("bullet"),
    slash: { id: "bullet-list", label: "slash.bullet" },
  },
  {
    description: "shortcut.number",
    key: "Ctrl-Shift-7",
    run: block("number"),
    slash: { id: "numbered-list", label: "slash.number" },
  },
  {
    description: "shortcut.task",
    key: "Ctrl-Shift-9",
    run: block("task"),
    slash: { id: "task-list", label: "slash.task" },
  },
  {
    description: "shortcut.quote",
    key: "Ctrl-Shift-.",
    run: block("quote"),
    slash: { id: "quote", label: "slash.quote" },
  },
];

export const formattingKeymap: KeyBinding[] = formattingShortcuts;
