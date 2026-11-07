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
import { drawSelection, EditorView, keymap } from "@codemirror/view";
import { codeHighlighter, codeLanguage } from "./code-colors";
import { formattingKeymap, inList } from "./formatting";
import { formattingBar } from "./formatting-bar";
import { liveFormatting } from "./live-formatting";
import { markdownExtensions } from "./markdown";
import { noteSearch, noteSearchKeymap } from "./note-search";
import { addPropertyShortcut, propertyCompletion } from "./properties";
import { tagDiagnostics } from "./tag-diagnostics";

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
  changes.iterChanges((from, to, _fromB, _toB, inserted) => {
    result += raw.slice(end, offsets[from]) + inserted.toString().replace(/\n/g, separator);
    end = offsets[to];
  });
  return result + raw.slice(end);
}

export function editorExtensions(
  onLeave: () => void,
  onFocus: (focused: boolean) => void,
  label: string,
  tags: () => string[] = () => [],
) {
  return [
    autocompletion({ override: [propertyCompletion(tags)], icons: false }),
    // Default pairs omit Markdown backticks; quotes after word characters stay literal.
    closeBrackets(),
    markdown({ extensions: markdownExtensions, addKeymap: false, codeLanguages: codeLanguage }),
    syntaxHighlighting(codeHighlighter),
    history(),
    noteSearch(),
    formattingBar(),
    // Several cursors: Alt+click adds one, Ctrl+D the next occurrence.
    EditorState.allowMultipleSelections.of(true),
    EditorView.clickAddsSelectionRange.of((event) => event.altKey),
    drawSelection(),
    keymap.of([
      ...formattingKeymap,
      {
        key: "Ctrl-e",
        run: () => {
          onLeave();
          return true;
        },
      },
      addPropertyShortcut,
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
    liveFormatting(),
    tagDiagnostics,
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
