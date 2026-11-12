import { quoteYaml } from "./markdown";
// Search syntax and completion ranges are pure; input focus belongs to the caller.
export type QueryToken = { kind: "tag" | "date"; value: string; from: number; to: number };
export type Query = { text: string; tokens: QueryToken[] };
const parts = /#"(?:\\.|[^"\\])*"(?=\s|$)|\S+/g;

/** Whitespace-delimited #tokens filter frontmatter tags; quoted tokens support spaces. */
export function parseQuery(query: string): Query {
  const tokens: QueryToken[] = [];
  const text: string[] = [];
  for (const match of query.matchAll(parts)) {
    const raw = match[0];
    if (raw.startsWith("date:")) {
      tokens.push({
        kind: "date",
        value: raw.slice(5),
        from: match.index,
        to: match.index + raw.length,
      });
      continue;
    }
    let value = raw.slice(1);
    if (raw.startsWith('#"')) {
      try {
        value = JSON.parse(raw.slice(1));
      } catch {
        value = "";
      }
    }
    if (
      raw.startsWith("#") &&
      value &&
      (raw.startsWith('#"') ? raw.endsWith('"') : !value.startsWith("#"))
    )
      tokens.push({ kind: "tag", value, from: match.index, to: match.index + raw.length });
    else text.push(raw);
  }
  return { text: text.join(" "), tokens };
}
export type QueryCompletion = { kind: "tag" | "date"; from: number; to: number; prefix: string };
/** Complete the whole token at a collapsed caret, preserving surrounding query text. */
export function queryCompletion(query: string, caret: number): QueryCompletion | null {
  for (const match of query.matchAll(parts)) {
    const from = match.index,
      to = from + match[0].length;
    if (caret > from + 4 && caret <= to && match[0].startsWith("date:"))
      return { kind: "date", from, to, prefix: query.slice(from + 5, caret) };
    if (caret <= from || caret > to || !match[0].startsWith("#") || match[0].startsWith("##"))
      continue;
    let prefix = query.slice(from + 1, caret);
    if (prefix.startsWith('"')) prefix = prefix.slice(1).replace(/"$/, "");
    return { kind: "tag", from, to, prefix };
  }
  return null;
}
export function tagToken(tag: string): string {
  return `#${/\s|"|\\/.test(tag) || tag.startsWith("#") ? JSON.stringify(tag) : tag}`;
}
export function applyCompletion(query: string, context: QueryCompletion, tag: string) {
  const token = completionToken(context.kind, tag);
  const suffix = query.slice(context.to);
  const insert = token + (suffix.startsWith(" ") ? "" : " ");
  return {
    text: query.slice(0, context.from) + insert + suffix,
    caret: context.from + token.length + 1,
  };
}
/** An unfinished leading # is also search intent, never an automatic note name. */
export function filterQuery(query: string): boolean {
  return query.trimStart().startsWith("#") || parseQuery(query).tokens.length > 0;
}
export function creationText(query: string): string {
  return filterQuery(query) ? parseQuery(query).text.replace(/^#$/, "") : query.trim();
}

export function completionToken(kind: QueryToken["kind"], value: string): string {
  return kind === "date" ? `date:${value}` : tagToken(value);
}
/** A sole complete token leaves the arrows and Enter to the note list. */
export function completionChoices(
  query: string,
  context: QueryCompletion,
  values: string[],
): string[] {
  return values.length === 1 &&
    query.slice(context.from, context.to) === completionToken(context.kind, values[0])
    ? []
    : values;
}
/** Creation seeds tag filters only; a new note keeps its current filesystem date. */
export function creationContent(query: string): string {
  const tags = [
    ...new Set(
      parseQuery(query)
        .tokens.filter((token) => token.kind === "tag")
        .map((token) => token.value),
    ),
  ];
  return tags.length ? `---\ntags: [${tags.map(quoteYaml).join(", ")}]\n---\n` : "";
}
