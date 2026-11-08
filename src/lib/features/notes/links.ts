import { decodeMarkdown, frontmatter, noteParser, safeHref } from "./markdown";

const SCHEME = /^[a-z][a-z\d+.-]*:/i;

const folders = (note: string) => note.split("/").slice(0, -1);
const percent = (c: string) =>
  encodeURIComponent(c).replace(/[()]/g, (p) => `%${p.charCodeAt(0).toString(16).toUpperCase()}`);

/**
 * The text of a note moved from `from` to `to`, with every relative link and image destination
 * (inline, and reference definitions) pointing at the same file from the new folder. Web, email and
 * `#` targets, absolute paths, paths that leave the notes folder and anything in code or the
 * frontmatter stay as they are; so does every other byte, line endings included. Raw HTML is not
 * touched: it shows as text (decision 0007).
 */
export function movedLinks(text: string, from: string, to: string): string {
  const before = folders(from);
  const after = folders(to);
  if (before.join("/").toLowerCase() === after.join("/").toLowerCase()) return text;
  // Same length as `text`, so offsets match: the parser sees `\n` line ends.
  const flat = text.replace(/\r(?!\n)/g, "\n").replace(/\r/g, " ");
  const lines = text.split(/\r\n?|\n/);
  const meta = frontmatter((n) => lines[n - 1], lines.length);
  let start = 0;
  if (meta) {
    const flatLines = flat.split("\n");
    for (let n = 0; n < meta.end; n++) start += flatLines[n].length + 1;
    start = Math.min(start, flat.length);
  }
  const edits: { from: number; to: number; text: string }[] = [];
  noteParser.parse(flat.slice(start)).iterate({
    enter(node) {
      if (node.name !== "URL") return;
      const parent = node.node.parent?.name ?? "";
      if (!/^(Link|Image|LinkReference)$/.test(parent)) return;
      const raw = text.slice(start + node.from, start + node.to);
      const next = movedTarget(raw, before, after);
      if (next !== null) edits.push({ from: start + node.from, to: start + node.to, text: next });
    },
  });
  let out = text;
  for (const edit of edits.reverse())
    out = out.slice(0, edit.from) + edit.text + out.slice(edit.to);
  return out;
}

/** `raw` as written, `<...>` or bare, pointing at the same file from `after`; null to keep it. */
function movedTarget(raw: string, before: string[], after: string[]): string | null {
  const angle = raw.startsWith("<");
  const body = angle ? raw.slice(1, -1) : raw;
  const target = decodeMarkdown(body);
  if (!target || safeHref(target) !== null || SCHEME.test(target)) return null;
  if (/^[/\\#?]/.test(target) || target.includes("\\")) return null;
  // The query or fragment stays exactly as written.
  const cut = body.search(/[?#]/);
  const suffix = cut < 0 ? "" : body.slice(cut);
  let path: string;
  try {
    const written = decodeMarkdown(cut < 0 ? body : body.slice(0, cut));
    path = decodeURIComponent(written.replace(/%(?![a-f\d]{2})/gi, "%25"));
  } catch {
    return null;
  }
  const parts = [...before];
  for (const part of path.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!parts.length) return null;
      parts.pop();
    } else parts.push(part);
  }
  let common = 0;
  while (
    common < after.length &&
    common < parts.length &&
    after[common].toLowerCase() === parts[common].toLowerCase()
  )
    common++;
  const trailing = path.endsWith("/") ? "/" : "";
  const moved =
    [...after.slice(common).map(() => ".."), ...parts.slice(common)].join("/") + trailing || ".";
  if (moved === path) return null;
  const encoded = Array.from(moved, (c) =>
    c < " " || "%<>".includes(c) || (!angle && " ()[]".includes(c)) ? percent(c) : c,
  ).join("");
  return angle ? `<${encoded}${suffix}>` : encoded + suffix;
}
