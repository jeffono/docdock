import { type EditorState, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView } from "@codemirror/view";
import { t } from "../../i18n";
import { tagProblems } from "./markdown";

function decorations(state: EditorState): DecorationSet {
  const { doc } = state;
  return Decoration.set(
    tagProblems((n) => doc.line(n).text, doc.lines).map((problem) =>
      Decoration.line({
        attributes: {
          class: "cm-tag-problem",
          title: t(problem.replacement === null ? "editor.tagsInvalid" : "editor.tagsRepair"),
        },
      }).range(doc.line(problem.line).from),
    ),
  );
}

/** Diagnostics never dispatch edits; repair is reserved for leaving the editor. */
export const tagDiagnostics = StateField.define<DecorationSet>({
  create: decorations,
  update: (value, transaction) => (transaction.docChanged ? decorations(transaction.state) : value),
  provide: (field) => EditorView.decorations.from(field),
});
