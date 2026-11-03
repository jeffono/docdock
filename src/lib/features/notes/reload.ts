import { invertedEffects, isolateHistory } from "@codemirror/commands";
import { type EditorState, StateEffect, StateField } from "@codemirror/state";
import { applyTextChanges } from "./editor";

const exactText = StateEffect.define<string>();
export const rawText = StateField.define<string>({
  create: (state) => state.doc.toString(),
  update(raw, transaction) {
    let next = transaction.docChanged ? applyTextChanges(raw, transaction.changes) : raw;
    // History joins inverse effects newest first; the last snapshot is the group's starting text.
    for (const effect of transaction.effects) {
      if (effect.is(exactText)) next = effect.value;
    }
    return next;
  },
});
export const rawHistory = invertedEffects.of((transaction) =>
  transaction.docChanged || transaction.effects.some((effect) => effect.is(exactText))
    ? [exactText.of(transaction.startState.field(rawText))]
    : [],
);

/** One isolated undo step; the effect preserves exact bytes, including newline-only changes. */
export function diskReload(state: EditorState, raw: string) {
  const before = state.doc.toString();
  const after = raw.replace(/\r\n?|\n/g, "\n");
  let from = 0;
  while (from < before.length && from < after.length && before[from] === after[from]) from++;
  let end = before.length;
  let nextEnd = after.length;
  while (end > from && nextEnd > from && before[end - 1] === after[nextEnd - 1]) {
    end--;
    nextEnd--;
  }
  return {
    changes: { from, to: end, insert: after.slice(from, nextEnd) },
    effects: exactText.of(raw),
    annotations: isolateHistory.of("full"),
  };
}
