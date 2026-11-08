import { decodeMarkdown, frontmatter, noteParser, safeHref } from "./markdown";

const SCHEME = /^[a-z][a-z\d+.-]*:/i;
/** Characters a destination segment cannot hold as themselves: Markdown or a URL reads them otherwise. */
const sensitive = (c: string) => c <= " " || c === "\u007f" || `"#%&'()*<>?[\\]^\`{|}`.includes(c);

const folders = (note: string) => note.split("/").slice(0, -1);

/**
 * The text of a note moved from `from` to `to`, with every relative link and image destination
 * (inline, and reference definitions) pointing at the same file from the new folder. Web, email and
 * `#` targets, absolute paths, paths that leave the notes folder and anything in code or the
 * frontmatter stay as they are; so does every other byte, line endings and a byte order mark
 * included. Raw HTML is not touched: it shows as text (decision 0007).
 */
export function movedLinks(text: string, from: string, to: string): string {
  const before = folders(from);
  const after = folders(to);
  if (before.join("/").toLowerCase() === after.join("/").toLowerCase()) return text;
  // Same length as `text`, so offsets match: the parser sees `\n` line ends.
  const flat = text.replace(/\r(?!\n)/g, "\n").replace(/\r/g, " ");
  // A byte order mark is ignored for detection only; the offsets below still count it.
  const lines = text.replace(/^﻿/, "").split(/\r\n?|\n/);
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

/** A segment as the reader gets it: Markdown escapes and entities, then percent-encoding. */
function segmentName(raw: string): string | null {
  try {
    return decodeURIComponent(decodeMarkdown(raw).replace(/%(?![a-f\d]{2})/gi, "%25"));
  } catch {
    return null;
  }
}

/**
 * `raw` as written, `<...>` or bare, pointing at the same file from `after`; null to keep it. Only the
 * leading `..` segments and the folders the note left change: the segments the link already had keep
 * their exact spelling, and folder names taken from the note path are percent-encoded wherever
 * Markdown or a URL would read a character differently.
 */
function movedTarget(raw: string, before: string[], after: string[]): string | null {
  const angle = raw.startsWith("<");
  const body = angle ? raw.slice(1, -1) : raw;
  const target = decodeMarkdown(body);
  if (!target || safeHref(target) !== null || SCHEME.test(target)) return null;
  if (/^[/\\#?]/.test(target) || target.includes("\\")) return null;
  // The query or fragment stays exactly as written.
  const cut = body.search(/[?#]/);
  const path = cut < 0 ? body : body.slice(0, cut);
  const suffix = body.slice(path.length);
  const parts = before.map((name) => ({ name, raw: encodeName(name) }));
  for (const segment of path.split("/")) {
    const name = segmentName(segment);
    if (name === null || name.includes("/")) return null;
    if (!name || name === ".") continue;
    if (name === "..") {
      if (!parts.length) return null;
      parts.pop();
    } else parts.push({ name, raw: segment });
  }
  let common = 0;
  while (
    common < after.length &&
    common < parts.length &&
    after[common].toLowerCase() === parts[common].name.toLowerCase()
  )
    common++;
  const trailing = path.endsWith("/") ? "/" : "";
  const ups = after.slice(common).map(() => "..");
  const moved = [...ups, ...parts.slice(common).map((p) => p.raw)].join("/") + trailing || ".";
  if (moved === path) return null;
  return angle ? `<${moved}${suffix}>` : moved + suffix;
}

/** A folder name as a destination segment that reads back as the same name. */
function encodeName(name: string): string {
  return Array.from(name, (c) =>
    sensitive(c)
      ? Array.from(
          new TextEncoder().encode(c),
          (b) => `%${b.toString(16).toUpperCase().padStart(2, "0")}`,
        ).join("")
      : c,
  ).join("");
}
