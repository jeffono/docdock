import { quoteYaml } from "./markdown";
import { type DateRange, dateRange } from "./query-date";
// Search syntax and completion ranges are pure; input focus belongs to the caller.
export type QueryToken = {
  kind: "tag" | "date" | "is" | "has";
  value: string;
  from: number;
  to: number;
};
export type Query = { text: string; tokens: QueryToken[] };
export const isPresets = ["link"] as const;
export const hasPresets = ["gallery"] as const;
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
    if (/^is:/i.test(raw)) {
      tokens.push({
        kind: "is",
        value: raw.slice(3),
        from: match.index,
        to: match.index + raw.length,
      });
      continue;
    }
    if (/^has:/i.test(raw)) {
      tokens.push({
        kind: "has",
        value: raw.slice(4),
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
export type QueryCompletion = {
  kind: QueryToken["kind"];
  from: number;
  to: number;
  prefix: string;
};
/** Complete the whole token at a collapsed caret, preserving surrounding query text. */
export function queryCompletion(query: string, caret: number): QueryCompletion | null {
  for (const match of query.matchAll(parts)) {
    const from = match.index,
      to = from + match[0].length;
    if (caret > from + 4 && caret <= to && match[0].startsWith("date:"))
      return { kind: "date", from, to, prefix: query.slice(from + 5, caret) };
    if (caret > from + 2 && caret <= to && /^is:/i.test(match[0]))
      return { kind: "is", from, to, prefix: query.slice(from + 3, caret) };
    if (caret > from + 3 && caret <= to && /^has:/i.test(match[0]))
      return { kind: "has", from, to, prefix: query.slice(from + 4, caret) };
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

/** Only free text from an opted-in note search is eligible for remote suggestions. */
export function noteSuggestionQuery(query: string, enabled = false): string | null {
  if (!enabled) return null;
  const { text } = parseQuery(query);
  // An unfinished or invalid quoted tag may leave parts of the tag in the parser's free text.
  if (/(?:^|\s)#"/.test(text)) return null;
  return Array.from(text).length >= 3 ? text : null;
}

export function completionToken(kind: QueryToken["kind"], value: string): string {
  switch (kind) {
    case "date":
      return `date:${value}`;
    case "is":
      return `is:${value}`;
    case "has":
      return `has:${value}`;
    case "tag":
      return tagToken(value);
  }
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

export function foldTag(value: string): string {
  return value.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

export type ActiveFilters = {
  link: boolean;
  gallery: boolean;
  dates: string[];
  tagKeys: Set<string>;
};

export type EffectiveFilters = {
  text: string;
  tags: string[];
  dates: DateRange[];
  needLink: boolean;
  needGallery: boolean;
};

export function effectiveFilters(
  query: string,
  now: Date = new Date(),
  completing: QueryCompletion | null = null,
): EffectiveFilters {
  const parsed = parseQuery(query);
  const tags: string[] = [];
  const dates: DateRange[] = [];
  let needLink = false;
  let needGallery = false;
  for (const token of parsed.tokens) {
    if (
      completing &&
      token.kind === completing.kind &&
      token.from === completing.from &&
      token.to === completing.to
    )
      continue;
    if (token.kind === "tag") tags.push(token.value);
    else if (token.kind === "date") {
      const range = dateRange(token.value, now);
      if (range) dates.push(range);
    } else if (token.kind === "is" && token.value.toLowerCase() === "link") {
      needLink = true;
    } else if (token.kind === "has" && token.value.toLowerCase() === "gallery") {
      needGallery = true;
    }
  }
  return { text: parsed.text, tags, dates, needLink, needGallery };
}

export function activeFilters(query: string, now: Date = new Date()): ActiveFilters {
  const dates: string[] = [];
  const tagKeys = new Set<string>();
  let link = false;
  let gallery = false;
  for (const token of parseQuery(query).tokens) {
    if (token.kind === "is" && token.value.toLowerCase() === "link") link = true;
    else if (token.kind === "has" && token.value.toLowerCase() === "gallery") gallery = true;
    else if (token.kind === "date" && dateRange(token.value, now) !== null)
      dates.push(token.value.toLowerCase());
    else if (token.kind === "tag") tagKeys.add(foldTag(token.value));
  }
  return { link, gallery, dates, tagKeys };
}

export function hasActiveFilters(query: string, now: Date = new Date()): boolean {
  const active = activeFilters(query, now);
  return active.link || active.gallery || active.dates.length > 0 || active.tagKeys.size > 0;
}

function removeTokens(query: string, tokens: QueryToken[]): string {
  if (!tokens.length) return query;
  const parts: string[] = [];
  let pos = 0;
  for (const token of tokens) {
    const piece = query.slice(pos, token.from).trim();
    if (piece) parts.push(piece);
    pos = token.to;
  }
  const tail = query.slice(pos).trim();
  if (tail) parts.push(tail);
  return parts.join(" ");
}

export function toggleFilterToken(
  query: string,
  kind: "is" | "has" | "tag",
  value: string,
): string {
  const key = kind === "tag" ? foldTag(value) : value.toLowerCase();
  const matches = parseQuery(query).tokens.filter(
    (token) =>
      token.kind === kind &&
      (kind === "tag" ? foldTag(token.value) === key : token.value.toLowerCase() === key),
  );
  if (matches.length) return removeTokens(query, matches);
  const token = completionToken(kind, value);
  const trimmed = query.trim();
  return trimmed ? `${trimmed} ${token}` : token;
}

export function setDateFilter(query: string, preset: string | null): string {
  const dateTokens = parseQuery(query).tokens.filter((token) => token.kind === "date");
  const target = preset?.toLowerCase() ?? null;
  const alreadySelected =
    target !== null && dateTokens.length === 1 && dateTokens[0].value.toLowerCase() === target;
  const base = removeTokens(query, dateTokens);
  if (preset === null || alreadySelected) return base;
  const token = completionToken("date", preset);
  return base ? `${base} ${token}` : token;
}

export function clearFilters(query: string): string {
  return parseQuery(query).text;
}
