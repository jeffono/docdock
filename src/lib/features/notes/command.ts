import type { EditorState, TransactionSpec } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

/** A slash command edits the state after removing its query, in the same transaction. */
export type CommandEdit = {
  state: EditorState;
  dispatch: (spec: TransactionSpec) => void;
};

export type EditorCommand = (view: EditorView, edit?: CommandEdit) => boolean;
