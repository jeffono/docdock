import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { syntaxTree } from "@codemirror/language";
import { type ChangeSet, EditorState } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, keymap, ViewPlugin } from "@codemirror/view";

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

function formatting(view: EditorView): DecorationSet {
  const ranges: ReturnType<Decoration["range"]>[] = [];
  const active = (from: number, to: number) =>
    view.state.selection.ranges.some(
      (r) => view.state.doc.lineAt(r.from).from <= to && view.state.doc.lineAt(r.to).to >= from,
    );
  syntaxTree(view.state).iterate({
    enter(node) {
      const { name } = node.type;
      const cls = /^ATXHeading|^SetextHeading/.test(name)
        ? "heading"
        : name === "StrongEmphasis"
          ? "strong"
          : name === "Emphasis"
            ? "emphasis"
            : name === "InlineCode" || name === "FencedCode" || name === "CodeBlock"
              ? "code"
              : name === "Link"
                ? "link"
                : "";
      if (cls) ranges.push(Decoration.mark({ class: cls }).range(node.from, node.to));
      // Never replace across lines: selection and block layout remain native CodeMirror behavior.
      if (
        /^(HeaderMark|EmphasisMark|CodeMark|LinkMark|URL)$/.test(name) &&
        !active(node.from, node.to) &&
        view.state.doc.lineAt(node.from).number === view.state.doc.lineAt(node.to).number
      ) {
        ranges.push(Decoration.replace({}).range(node.from, node.to));
      }
    },
  });
  return Decoration.set(ranges, true);
}

export function editorExtensions(
  onChange: (changes: ChangeSet) => void,
  onLeave: () => void,
  onFocus: (focused: boolean) => void,
  label: string,
) {
  return [
    markdown(),
    history(),
    keymap.of([...defaultKeymap, ...historyKeymap]),
    EditorView.lineWrapping,
    EditorView.contentAttributes.of({ "aria-label": label }),
    ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;
        constructor(view: EditorView) {
          this.decorations = formatting(view);
        }
        update(update: import("@codemirror/view").ViewUpdate) {
          this.decorations = formatting(update.view);
        }
      },
      { decorations: (value) => value.decorations },
    ),
    EditorView.updateListener.of((update) => {
      if (update.docChanged) onChange(update.changes);
    }),
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
