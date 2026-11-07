import { parseAddress } from "./address";

export type WebEntry = {
  key: string;
  kind: "open" | "search" | "intent";
  text: string;
  url?: string;
};
export function webQuery(query: string, browser: boolean): string | null {
  return browser || query.trimStart().startsWith("?") ? query.trim().replace(/^\?\s*/, "") : null;
}
/** Recent addresses already include searches as their locally stored result URLs. */
export function historyEntries(history: string[]): WebEntry[] {
  return history.slice(0, 5).map((url) => ({
    key: `web:history:${url}`,
    kind: "open",
    text: url,
    url,
  }));
}
export function webEntries(
  query: string,
  browser: boolean,
  history: string[],
  remote: string[],
): WebEntry[] {
  const intent = webQuery(query, browser);
  const text = (intent ?? query).trim();
  if (!text) return [];
  const url = parseAddress(text);
  const primary: WebEntry = url
    ? { key: "web:open", kind: "open", text: url, url }
    : { key: "web:search", kind: intent === null ? "intent" : "search", text };
  if (intent === null) return [primary];
  const local = history.filter((url) => url.toLowerCase().includes(text.toLowerCase())).slice(0, 5);
  return [
    primary,
    ...local.map((url): WebEntry => ({ key: `web:history:${url}`, kind: "open", text: url, url })),
    ...remote
      .filter((s) => s !== text)
      .map((s): WebEntry => ({ key: `web:search:${s}`, kind: "search", text: s })),
  ];
}
export function barDefault(
  query: string,
  browser: boolean,
  noteDefault: string,
  entries: WebEntry[],
): string {
  return browser || webQuery(query, false) !== null || /^https?:\/\//i.test(query.trim())
    ? (entries[0]?.key ?? "")
    : noteDefault || entries[0]?.key || "";
}
/** A generation, rather than text equality, also rejects an old response after A → B → A. */
export class LatestQuery {
  private revision = 0;
  next(): number {
    return ++this.revision;
  }
  accepts(revision: number): boolean {
    return revision === this.revision;
  }
}
export function searchAddress(text: string): string {
  return `https://duckduckgo.com/?q=${encodeURIComponent(text)}`;
}

/** Down in Notes must never replace the note selection with URL history. */
export function downOpensHistory(browser: boolean, suggestionCount: number): boolean {
  return browser && suggestionCount === 0;
}
