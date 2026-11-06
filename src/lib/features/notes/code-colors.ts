import { LanguageDescription, LanguageSupport, LRLanguage } from "@codemirror/language";
import type { NodeType } from "@lezer/common";
import { highlightTree, tagHighlighter, tags } from "@lezer/highlight";

const tops = new Set<NodeType>();
function support(parser: LRLanguage["parser"]) {
  const language = LRLanguage.define({ parser });
  tops.add(language.parser.topNode);
  return new LanguageSupport(language);
}

// The same cached, local grammar chunks serve both views; no user input becomes a module path.
const languages = [
  LanguageDescription.of({
    name: "javascript",
    alias: ["js"],
    load: async () => support((await import("@lezer/javascript")).parser),
  }),
  LanguageDescription.of({
    name: "typescript",
    alias: ["ts"],
    load: async () =>
      support((await import("@lezer/javascript")).parser.configure({ dialect: "ts" })),
  }),
  LanguageDescription.of({
    name: "jsx",
    load: async () =>
      support((await import("@lezer/javascript")).parser.configure({ dialect: "jsx" })),
  }),
  LanguageDescription.of({
    name: "tsx",
    load: async () =>
      support((await import("@lezer/javascript")).parser.configure({ dialect: "ts jsx" })),
  }),
  LanguageDescription.of({
    name: "json",
    load: async () => support((await import("@lezer/json")).parser),
  }),
  LanguageDescription.of({
    name: "css",
    load: async () => support((await import("@lezer/css")).parser),
  }),
  LanguageDescription.of({
    name: "html",
    load: async () => support((await import("@lezer/html")).parser),
  }),
];

export function codeLanguage(info: string) {
  return LanguageDescription.matchLanguageName(languages, info.trim().split(/\s/, 1)[0], false);
}

export const codeHighlighter = tagHighlighter(
  [
    { tag: tags.comment, class: "code-comment" },
    { tag: [tags.keyword, tags.typeName, tags.tagName], class: "code-keyword" },
    { tag: [tags.string, tags.attributeValue], class: "code-string" },
    { tag: [tags.number, tags.bool, tags.null], class: "code-literal" },
  ],
  { scope: (node) => tops.has(node) },
);

export type CodeSpan = { from: number; to: number; className: string };
/** Call after loading the description; unknown or unavailable grammars stay plain. */
export function codeSpans(text: string, info: string): CodeSpan[] {
  const language = codeLanguage(info)?.support?.language;
  if (!language) return [];
  const spans: CodeSpan[] = [];
  highlightTree(language.parser.parse(text), codeHighlighter, (from, to, className) => {
    spans.push({ from, to, className });
  });
  return spans;
}
