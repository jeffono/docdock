// Frontmatter in the editor: key and tag completion inside the raw block, and Ctrl+; to add a property.
import {
  type Completion,
  type CompletionContext,
  type CompletionResult,
  startCompletion,
} from "@codemirror/autocomplete";
import type { EditorView } from "@codemirror/view";
import { frontmatter, yamlValue } from "./markdown";

/** Keys most notes tools read; the card uses title and tags, page notes url. */
export const commonKeys = [
  "title",
  "tags",
  "aliases",
  "date",
  "author",
  "description",
  "status",
  "url",
] as const;

export type PropertyContext = {
  kind: "key" | "tag";
  /** Columns the completion replaces. */
  from: number;
  to: number;
  /** A key already followed by its colon keeps it. */
  colon: boolean;
  used: Set<string>;
};

/**
 * What can be completed at column `column` of line `at` (1-based) of a note, from the text before it:
 * a key at the start of a frontmatter line, or a tag in `tags: [a, b]`, `tags: a` or a `- a` item
 * under `tags:`. A key is replaced whole, also after the cursor. Null outside the block.
 */
export function propertyContext(
  line: (n: number) => string,
  lines: number,
  at: number,
  column: number,
): PropertyContext | null {
  const meta = frontmatter(line, lines);
  if (!meta || at < 2 || at >= meta.end) return null;
  const before = line(at).slice(0, column);
  const keyOf = (n: number) => line(n).match(/^([\w-]+):/)?.[1];
  if (/^[\w-]*$/.test(before)) {
    const used = new Set<string>();
    for (let n = 2; n < meta.end; n++) if (n !== at && keyOf(n)) used.add(keyOf(n) as string);
    const rest = line(at).slice(column);
    const word = rest.match(/^[\w-]*/)?.[0] ?? "";
    const colon = rest.slice(word.length).startsWith(":");
    return { kind: "key", from: 0, to: column + word.length, colon, used };
  }
  const used = new Set(meta.tags);
  const inline = before.match(/^tags?:\s*(?:\[(?:[^\],]*,\s*)*)?\s*([^\],[]*)$/);
  const tag = (word: string): PropertyContext => {
    return { kind: "tag", from: column - word.length, to: column, colon: false, used };
  };
  if (inline) return tag(inline[1]);
  const item = before.match(/^\s+-\s*(\S*)$/);
  if (!item) return null;
  for (let n = at - 1; n > 1; n--) {
    if (/^\s/.test(line(n))) continue;
    return /^tags?:\s*$/.test(line(n)) ? tag(item[1]) : null;
  }
  return null;
}

/** Completes keys and the folder's tags inside the frontmatter block. */
export function propertyCompletion(tags: () => string[]) {
  return (context: CompletionContext): CompletionResult | null => {
    const { doc } = context.state;
    const line = doc.lineAt(context.pos);
    const found = propertyContext(
      (n) => doc.line(n).text,
      doc.lines,
      line.number,
      context.pos - line.from,
    );
    if (!found) return null;
    const from = line.from + found.from;
    if (from === context.pos && !context.explicit) return null;
    const options: Completion[] =
      found.kind === "key"
        ? commonKeys
            .filter((key) => !found.used.has(key))
            .map((key) => ({ label: key, apply: found.colon ? key : `${key}: `, type: "property" }))
        : [...new Set(tags())]
            .filter((tag) => !found.used.has(tag))
            .sort((a, b) => a.localeCompare(b))
            .map((tag) => ({ label: tag, apply: yamlValue(tag), type: "keyword" }));
    const validFor = found.kind === "key" ? /^[\w-]*$/ : /^[^,[\]]*$/;
    const to = line.from + found.to;
    return options.length ? { from, to, options, validFor } : null;
  };
}

/**
 * Ctrl+;: a new empty line at the end of the frontmatter, creating the block when there is none, with
 * the key list open on it.
 */
export function addProperty(view: EditorView): boolean {
  // Read-only while an image is pasted: its insertion point must not move.
  if (view.state.readOnly) return true;
  const { doc } = view.state;
  const meta = frontmatter((n) => doc.line(n).text, doc.lines);
  if (meta) {
    const at = doc.line(meta.end - 1).to;
    view.dispatch({ changes: { from: at, insert: "\n" }, selection: { anchor: at + 1 } });
  } else {
    const from = doc.sliceString(0, 1) === "﻿" ? 1 : 0;
    view.dispatch({ changes: { from, insert: "---\n\n---\n" }, selection: { anchor: from + 4 } });
  }
  view.dispatch({ scrollIntoView: true });
  startCompletion(view);
  return true;
}
