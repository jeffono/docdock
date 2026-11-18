import {
  type CompletionContext,
  type CompletionResult,
  pickedCompletion,
} from "@codemirror/autocomplete";
import { isolateHistory } from "@codemirror/commands";
import { ensureSyntaxTree } from "@codemirror/language";
import type { TransactionSpec } from "@codemirror/state";
import type { SyntaxNode } from "@lezer/common";
import { type MessageKey, t } from "../../i18n";
import type { EditorCommand } from "./command";
import { noteBodyStart } from "./count";
import { formattingShortcuts } from "./formatting";
import { addPropertyShortcut } from "./properties";
import { textTransforms } from "./transforms";

type SlashCommand = {
  id: string;
  label: MessageKey;
  detail?: MessageKey;
  level?: number;
  run: EditorCommand;
};

function insertBlock(text: string, cursor: number, blankBefore = false): EditorCommand {
  return (view, edit = view) => {
    const { doc, selection } = edit.state;
    const at = selection.main.head;
    const line = doc.lineAt(at);
    const before = doc.sliceString(line.from, at);
    const previous = line.number > 1 ? doc.line(line.number - 1).text : "---";
    // A blank line also keeps a divider at the start of a note out of frontmatter.
    const prefix = (before ? "\n" : "") + (blankBefore && (before || previous).trim() ? "\n" : "");
    const suffix = at < line.to ? "\n" : "";
    edit.dispatch({
      changes: { from: at, insert: `${prefix}${text}${suffix}` },
      selection: { anchor: at + prefix.length + cursor },
    });
    return true;
  };
}

const insertDate: EditorCommand = (view, edit = view) => {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  const date = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
  const at = edit.state.selection.main.head;
  edit.dispatch({ changes: { from: at, insert: date }, selection: { anchor: at + date.length } });
  return true;
};

const slashCommands: SlashCommand[] = [
  ...formattingShortcuts.flatMap(({ slash, level, run }) =>
    slash ? [{ ...slash, level, run }] : [],
  ),
  { id: "code-block", label: "slash.codeBlock", run: insertBlock("```\n\n```", 4) },
  { id: "divider", label: "slash.divider", run: insertBlock("---", 3, true) },
  {
    id: "add-property",
    label: "slash.addProperty",
    detail: "slash.addPropertyDetail",
    run: addPropertyShortcut.run,
  },
  { id: "todays-date", label: "slash.today", run: insertDate },
  ...textTransforms.map(
    ({ id, label, run }) =>
      ({
        id,
        label,
        run: (view, edit) => run(view, edit, "line-or-block"),
      }) satisfies SlashCommand,
  ),
];

/** The commands that make sense inside the frontmatter's YAML. */
const inFrontmatter = new Set([
  "add-property",
  "todays-date",
  ...textTransforms.map(({ id }) => id),
]);
const excludedNodes = new Set(["FencedCode", "CodeBlock", "InlineCode", "URL", "Autolink"]);
const normalize = (value: string) => value.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

export function slashCompletion(context: CompletionContext): CompletionResult | null {
  const { state, pos } = context;
  if (state.readOnly) return null;
  const line = state.doc.lineAt(pos);
  const before = state.sliceDoc(line.from, pos);
  const offset = before.lastIndexOf("/");
  if (offset < 0 || (offset > 0 && !/\s/.test(before[offset - 1]))) return null;
  const from = line.from + offset;
  const query = normalize(before.slice(offset + 1));
  if (/^\s/.test(query)) return null;
  const frontmatter = from < noteBodyStart(state.doc);
  if (!frontmatter) {
    const tree = ensureSyntaxTree(state, pos);
    if (!tree) return null;
    for (let node: SyntaxNode | null = tree.resolveInner(from, 1); node; node = node.parent) {
      if (excludedNodes.has(node.name)) return null;
    }
  }

  const options = slashCommands.flatMap(({ id, label, detail, level, run }) => {
    if (frontmatter && !inFrontmatter.has(id)) return [];
    const display = t(label, { level });
    if (!normalize(display).includes(query) && !id.includes(query)) return [];
    return [
      {
        label: display,
        detail: detail && t(detail),
        type: "command",
        apply(view, completion, from, to) {
          if (view.state.readOnly) return;
          const prefix = {
            changes: { from, to },
            selection: { anchor: from },
            annotations: [pickedCompletion.of(completion), isolateHistory.of("full")],
            userEvent: "input.complete",
          };
          const dispatch = (spec: TransactionSpec) =>
            view.dispatch(view.state.update(prefix, { ...spec, sequential: true }));
          if (!run(view, { state: view.state.update(prefix).state, dispatch })) dispatch({});
        },
      } satisfies CompletionResult["options"][number],
    ];
  });
  return options.length
    ? { from, options, filter: false, update: (_result, _from, _to, next) => slashCompletion(next) }
    : null;
}
