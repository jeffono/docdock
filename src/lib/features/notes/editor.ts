import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionStatus,
} from "@codemirror/autocomplete";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentLess,
  indentMore,
} from "@codemirror/commands";
import {
  deleteMarkupBackward,
  insertNewlineContinueMarkup,
  markdown,
} from "@codemirror/lang-markdown";
import { syntaxHighlighting } from "@codemirror/language";
import { searchPanelOpen } from "@codemirror/search";
import { type ChangeSet, EditorState } from "@codemirror/state";
import { drawSelection, EditorView, keymap, placeholder, tooltips } from "@codemirror/view";
import { panelColumns } from "../../ui/place";
import { codeHighlighter, codeLanguage } from "./code-colors";
import { formattingKeymap, inList } from "./formatting";
import { formattingBar } from "./formatting-bar";
import { frontmatterEnabled, liveFormatting } from "./live-formatting";
import { markdownExtensions } from "./markdown";
import { noteSearch, noteSearchKeymap } from "./note-search";
import { addPropertyShortcut, propertyCompletion } from "./properties";
import { slashCompletion } from "./slash-commands";
import { tagDiagnostics } from "./tag-diagnostics";
import { cycleCaseCommand } from "./transforms";

/** Apply normalized editor offsets to the original bytes; untouched mixed line endings survive. */
export function applyTextChanges(raw: string, changes: ChangeSet): string {
  const offsets: number[] = [0];
  for (let i = 0; i < raw.length; i++) {
    if (raw[i] === "\r" && raw[i + 1] === "\n") i++;
    offsets.push(i + 1);
  }
  const separator = raw.match(/\r\n|\r|\n/)?.[0] ?? "\n";
  let result = "",
    end = 0;
  // A lone CR break meeting an LF break, around a line that became empty, would read as one CRLF
  // break and join two lines: the CR becomes LF, keeping both breaks.
  const append = (piece: string) => {
    if (piece.startsWith("\n") && result.endsWith("\r")) result = `${result.slice(0, -1)}\n`;
    result += piece;
  };
  changes.iterChanges((from, to, _fromB, _toB, inserted) => {
    append(raw.slice(end, offsets[from]));
    append(inserted.toString().replace(/\n/g, separator));
    end = offsets[to];
  });
  append(raw.slice(end));
  return result;
}

export type EditorOptions = {
  frontmatter?: boolean;
  leaveOnCtrlE?: boolean;
  placeholder?: string;
};

export function editorExtensions(
  onLeave: () => void,
  onFocus: (focused: boolean) => void,
  label: string,
  tags: () => string[] = () => [],
  options: EditorOptions = {},
) {
  const frontmatter = options.frontmatter ?? true;
  const leaveOnCtrlE = options.leaveOnCtrlE ?? true;
  return [
    frontmatterEnabled.of(frontmatter),
    autocompletion({
      override: frontmatter ? [propertyCompletion(tags), slashCompletion] : [slashCompletion],
      icons: false,
      // Commands read as interface text; property keys and tags stay monospace like the YAML.
      optionClass: (completion) => (completion.type === "command" ? "cm-command-option" : ""),
    }),
    // Kept to the panel, not the whole viewport, which reaches beyond it beside the edge tab.
    tooltips({
      tooltipSpace: (view) => ({ ...panelColumns(view.dom), top: 0, bottom: innerHeight }),
    }),
    // Default pairs omit Markdown backticks; quotes after word characters stay literal.
    closeBrackets(),
    markdown({ extensions: markdownExtensions, addKeymap: false, codeLanguages: codeLanguage }),
    syntaxHighlighting(codeHighlighter),
    history(),
    noteSearch(),
    formattingBar(frontmatter),
    // Several cursors: Alt+click adds one, Ctrl+D the next occurrence.
    EditorState.allowMultipleSelections.of(true),
    EditorView.clickAddsSelectionRange.of((event) => event.altKey),
    drawSelection(),
    keymap.of([
      ...formattingKeymap,
      // "Alt-A" is how CodeMirror names Alt+Shift+A; it replaces the default block comment toggle.
      // Taken even with nothing to change, or the default would insert an HTML comment.
      {
        key: "Alt-A",
        run: (view) => {
          cycleCaseCommand(view);
          return true;
        },
      },
      ...(leaveOnCtrlE
        ? [
            {
              key: "Ctrl-e",
              run: () => {
                onLeave();
                return true;
              },
            },
          ]
        : []),
      ...(frontmatter ? [addPropertyShortcut] : []),
      { key: "Enter", run: insertNewlineContinueMarkup },
      // Empty pairs first; otherwise let Markdown remove list/quote markup.
      ...closeBracketsKeymap,
      { key: "Backspace", run: deleteMarkupBackward },
      { key: "Tab", run: (view) => inList(view.state) && indentMore(view) },
      { key: "Shift-Tab", run: (view) => inList(view.state) && indentLess(view) },
      ...noteSearchKeymap,
      ...defaultKeymap,
      ...historyKeymap,
    ]),
    EditorView.lineWrapping,
    EditorView.contentAttributes.of({ "aria-label": label, class: "note-content" }),
    ...(options.placeholder ? [placeholder(options.placeholder)] : []),
    liveFormatting(),
    ...(frontmatter ? [tagDiagnostics] : []),
    EditorView.domEventHandlers({
      focus: () => {
        onFocus(true);
      },
      blur: () => {
        onFocus(false);
      },
      keydown: (event, view) => {
        // Esc first closes an open completion list, then the search panel.
        if (
          event.key !== "Escape" ||
          completionStatus(view.state) === "active" ||
          searchPanelOpen(view.state)
        )
          return false;
        event.preventDefault();
        event.stopPropagation();
        onLeave();
        return true;
      },
    }),
  ];
}

export function editorState(text: string, extensions: ReturnType<typeof editorExtensions>) {
  return EditorState.create({ doc: text, extensions });
}
