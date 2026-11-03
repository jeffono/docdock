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
import { type ChangeSet, EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { GFM } from "@lezer/markdown";
import { formattingKeymap, inList } from "./formatting";
import { liveFormatting } from "./live-formatting";

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
) {
  return [
    markdown({ extensions: GFM, addKeymap: false }),
    history(),
    keymap.of([
      ...formattingKeymap,
      {
        key: "Ctrl-e",
        run: () => {
          onLeave();
          return true;
        },
      },
      { key: "Enter", run: insertNewlineContinueMarkup },
      { key: "Backspace", run: deleteMarkupBackward },
      { key: "Tab", run: (view) => inList(view.state) && indentMore(view) },
      { key: "Shift-Tab", run: (view) => inList(view.state) && indentLess(view) },
      ...defaultKeymap,
      ...historyKeymap,
    ]),
    EditorView.lineWrapping,
    EditorView.contentAttributes.of({ "aria-label": label, class: "note-content" }),
    liveFormatting(),
    EditorView.domEventHandlers({
      focus: () => {
        onFocus(true);
      },
      blur: () => {
        onFocus(false);
      },
      keydown: (event) => {
        if (event.key !== "Escape") return false;
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
