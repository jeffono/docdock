// Markdown rules shared by the reading view and the editor: one parser, one rule for each construct.
import type { SyntaxNode } from "@lezer/common";
import { GFM, type MarkdownConfig, parser } from "@lezer/markdown";

const footnoteStart = /^\[\^([^\]\s]+)\]:/;

/**
 * GitHub-style footnotes: `[^label]` references and `[^label]: text` definitions. A definition is a
 * paragraph, so lazy continuation lines belong to it; another definition line ends it.
 */
const Footnotes: MarkdownConfig = {
  defineNodes: [{ name: "FootnoteDefinition", block: true }, "FootnoteMark", "FootnoteReference"],
  parseBlock: [
    {
      name: "FootnoteDefinition",
      before: "LinkReference",
      leaf: (_, leaf) =>
        footnoteStart.test(leaf.content)
          ? {
              nextLine(_, __, leaf) {
                // A definition is never a link reference, setext heading or table: drop the
                // parsers that could otherwise claim its lines before `finish`.
                leaf.parsers.splice(leaf.parsers.indexOf(this) + 1);
                return false;
              },
              finish(cx, leaf) {
                const mark = leaf.content.match(footnoteStart)?.[0].length ?? 0;
                cx.addLeafElement(
                  leaf,
                  cx.elt("FootnoteDefinition", leaf.start, leaf.start + leaf.content.length, [
                    cx.elt("FootnoteMark", leaf.start, leaf.start + mark),
                    ...cx.parser.parseInline(leaf.content.slice(mark), leaf.start + mark),
                  ]),
                );
                return true;
              },
            }
          : null,
      endLeaf: (_, line) => footnoteStart.test(line.text.slice(line.pos)),
    },
  ],
  parseInline: [
    {
      name: "FootnoteReference",
      before: "Link",
      parse(cx, next, pos) {
        if (next !== 91 || cx.char(pos + 1) !== 94) return -1;
        // Labels hold no brackets or spaces and are short, so a scan never revisits the rest of an
        // unterminated text.
        let end = pos + 2;
        while (
          end < cx.end &&
          end - pos < 1000 &&
          !/[\s[\]]/.test(String.fromCharCode(cx.char(end)))
        )
          end++;
        if (end === pos + 2 || cx.char(end) !== 93) return -1;
        return cx.addElement(cx.elt("FootnoteReference", pos, end + 1));
      },
    },
  ],
};

export const markdownExtensions = [GFM, Footnotes];
export const noteParser = parser.configure(markdownExtensions);

type Slice = (from: number, to: number) => string;

/** Containers nested deeper than this render as text: deep recursion would overflow the stack. */
export const maxDepth = 32;

const named: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: String.fromCharCode(0xa0),
};
let entities: Document | null = null;
/** A character reference; other names are decoded by the browser in an inert document. */
export function decodeEntity(entity: string): string {
  const match = entity.match(/^&(?:#(\d{1,7})|#x([\da-f]{1,6})|([a-z][a-z\d]{1,31}));$/i);
  if (!match) return entity;
  if (match[1] || match[2]) {
    const code = Number.parseInt(match[1] ?? match[2], match[1] ? 10 : 16);
    return code > 0 && code <= 0x10ffff && (code < 0xd800 || code > 0xdfff)
      ? String.fromCodePoint(code)
      : String.fromCharCode(0xfffd);
  }
  if (named[match[3]]) return named[match[3]];
  if (typeof DOMParser === "undefined") return entity;
  entities ??= new DOMParser().parseFromString("", "text/html");
  const holder = entities.createElement("p");
  // Safe: the string matched a single named reference, with no markup characters.
  holder.innerHTML = entity;
  return holder.textContent || entity;
}

/** Backslash escapes and character references, as CommonMark decodes destinations and titles. */
export function decodeMarkdown(text: string): string {
  return text.replace(
    /\\([!-/:-@[-`{-~])|&(?:#\d{1,7}|#x[\da-f]{1,6}|[a-z][a-z\d]{1,31});/gi,
    (all, escaped) => escaped ?? decodeEntity(all),
  );
}

/** `[^label]` or `[^label]:` → label. */
export function footnoteLabel(syntax: string): string {
  return syntax.replace(/^\[\^|\]:?$/g, "");
}

/** CommonMark label matching: case-insensitive, inner whitespace collapsed. */
function normalizeLabel(label: string): string {
  return label.trim().replace(/\s+/g, " ").toLowerCase();
}

export type Definitions = {
  links: Map<string, { url: string; title: string }>;
  footnotes: Set<string>;
};

/** Link reference and footnote definitions, from block containers only. */
export function definitions(top: SyntaxNode, slice: Slice): Definitions {
  const found: Definitions = { links: new Map(), footnotes: new Set() };
  const visit = (node: SyntaxNode, depth: number) => {
    if (depth > maxDepth) return;
    for (let child = node.firstChild; child; child = child.nextSibling) {
      if (child.name === "LinkReference") {
        const label = child.getChild("LinkLabel"),
          url = child.getChild("URL"),
          title = child.getChild("LinkTitle");
        const key = label && normalizeLabel(slice(label.from + 1, label.to - 1));
        if (key && url && !found.links.has(key))
          found.links.set(key, {
            url: decodeMarkdown(slice(url.from, url.to).replace(/^<|>$/g, "")),
            title: title ? decodeMarkdown(slice(title.from + 1, title.to - 1)) : "",
          });
      } else if (child.name === "FootnoteDefinition") {
        const mark = child.getChild("FootnoteMark");
        if (mark) found.footnotes.add(footnoteLabel(slice(mark.from, mark.to)));
      } else if (/^(Blockquote|BulletList|OrderedList|ListItem)$/.test(child.name))
        visit(child, depth + 1);
    }
  };
  visit(top, 0);
  return found;
}

/**
 * Destination of a Link or Image node: inline, full reference `[text][label]` or shortcut `[text]`.
 * Null when nothing defines it, in which case CommonMark shows the brackets as text.
 */
export function linkTarget(
  node: SyntaxNode,
  slice: Slice,
  defs: Definitions,
): { url: string; title: string } | null {
  const url = node.getChild("URL");
  if (url) {
    const title = node.getChild("LinkTitle");
    return {
      url: decodeMarkdown(slice(url.from, url.to).replace(/^<|>$/g, "")),
      title: title ? decodeMarkdown(slice(title.from + 1, title.to - 1)) : "",
    };
  }
  const label = node.getChild("LinkLabel");
  const marks = node.getChildren("LinkMark");
  // A collapsed reference `[text][]` uses the text as its label.
  const own = label ? slice(label.from + 1, label.to - 1) : "";
  const text = own.trim() ? own : marks.length >= 2 ? slice(marks[0].to, marks[1].from) : "";
  return defs.links.get(normalizeLabel(text)) ?? null;
}

/** Links leave the app only for web and email; `#` targets stay inside the note. */
export function safeHref(url: string): string | null {
  const href = url.trim();
  if (/^(https?:\/\/|mailto:)/i.test(href) || /^#[^#]*$/.test(href)) return href;
  if (/^www\./i.test(href)) return `http://${href}`;
  if (/^[^\s@:/]+@[^\s@:/]+\.[^\s@:/]+$/.test(href)) return `mailto:${href}`;
  return null;
}

/**
 * Local image address inside the notes folder, relative to the note. Mirrors the `note-asset`
 * protocol: percent-decoded, no scheme, query, fragment or backslash, never above the folder.
 */
export function imageUrl(source: string, note: string): string | null {
  let src: string;
  try {
    src = decodeURIComponent(source.replace(/%(?![a-f\d]{2})/gi, "%25"));
  } catch {
    return null;
  }
  if (!src || src.startsWith("/") || /[:\\?#]/.test(src)) return null;
  const parts = note.split("/");
  parts.pop();
  for (const part of src.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!parts.length) return null;
      parts.pop();
    } else parts.push(part);
  }
  return `http://note-asset.localhost/${parts.map((p) => encodeURIComponent(p).replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`)).join("/")}`;
}

/** GitHub alert kind, from the text of a quote's first line starting at its `>`. */
export function alertKind(firstLine: string): string | null {
  return (
    firstLine.match(/^> ?\[!(note|tip|important|warning|caution)\][ \t]*$/i)?.[1].toLowerCase() ??
    null
  );
}

/** The supported YAML subset treats an unquoted `#` after whitespace as a comment. */
function scalar(raw: string): string | null {
  const value = raw.trim();
  if (value.startsWith('"')) {
    const match = value.match(/^"(?:[^"\\]|\\.)*"/);
    if (!match || !/^(?:#.*)?$/.test(value.slice(match[0].length).trim())) return null;
    try {
      return JSON.parse(match[0]) || null;
    } catch {
      return null;
    }
  }
  if (value.startsWith("'")) {
    const match = value.match(/^'((?:[^']|'')*)'(?:\s*#.*)?$/);
    return match ? match[1].replace(/''/g, "'") || null : null;
  }
  const result = value.replace(/(^|\s)#.*$/, "").trim();
  return !result || /^[[{|>\-&*!%@`]/.test(result) || /["'[\]{},]|: /.test(result) ? null : result;
}

/** JSON strings are YAML scalars; also escape YAML's extra line breaks and controls. */
export function quoteYaml(text: string): string {
  return JSON.stringify(text).replace(
    /[\u007f-\u009f\u2028\u2029]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
}

/**
 * `text` with its frontmatter title set to `title`, or null when the frontmatter has no title this
 * reader understands, so the card shows the file name instead. Only that line changes, with its own
 * terminator. The value stays unquoted when it reads back the same and is not another YAML type.
 * With `create`, a missing or unreadable title is written too, in a new frontmatter block if needed,
 * with the text's first line terminator.
 */
export function retitle(text: string, title: string, create = false): string | null {
  // A byte order mark stays first, before any new block.
  if (text.startsWith("﻿")) {
    const rest = retitle(text.slice(1), title, create);
    return rest === null ? null : `﻿${rest}`;
  }
  const lines = text.split(/(?<=\r\n|\n|\r(?!\n))/);
  const bare = (n: number) => lines[n - 1].replace(/\r?\n$|\r$/, "");
  const meta = frontmatter(bare, lines.length);
  if (!meta?.title && !create) return null;
  const line = `title: ${yamlValue(title)}`;
  if (!meta) {
    const newline = text.match(/\r\n|\n|\r/)?.[0] ?? "\n";
    return `---${newline}${line}${newline}---${newline}${text}`;
  }
  let at = 0;
  for (let n = 2; n < meta.end; n++) if (/^title:/.test(bare(n))) at = n;
  // A new title goes first in the block, ending as the opening line does.
  if (!at) {
    lines.splice(1, 0, `${line}${lines[0].slice(3)}`);
    return lines.join("");
  }
  // The whole old value goes, with its indented lines (a list or a block scalar), but not the blank
  // lines after it.
  let last = at;
  for (let n = at + 1; n < meta.end && /^(?:[ \t]|$)/.test(bare(n)); n++)
    if (bare(n).trim()) last = n;
  const end = lines[last - 1].slice(bare(last).length);
  lines.splice(at - 1, last - at + 1, `${line}${end}`);
  return lines.join("");
}

/** `text` as a YAML value: plain when it reads back the same and is not another type, else quoted. */
export function yamlValue(text: string): string {
  const plain =
    scalar(text) === text && !/^(?:true|false|yes|no|on|off|null|~|[-+]?[\d.].*)$/i.test(text);
  return plain ? text : quoteYaml(text);
}

/** Split a flat flow list without splitting commas inside quoted scalars. */
function tagItems(text: string): string[] | null {
  const items: string[] = [];
  let start = 0;
  let quote = "";
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === "\\" && quote === '"') i++;
      else if (c === quote) {
        if (quote === "'" && text[i + 1] === "'") i++;
        else quote = "";
      }
    } else if (c === '"' || c === "'") quote = c;
    else if (c === ",") {
      items.push(text.slice(start, i));
      start = i + 1;
    }
  }
  if (quote) return null;
  items.push(text.slice(start));
  return items;
}

/** Undefined means valid; null means ambiguous. Only plain word lists and flow colon items repair. */
function tagRepair(value: string): string | null | undefined {
  if (!value || scalar(value) !== null) return undefined;
  const words = /^[\p{L}\p{N}_-]+(?:[ \t]+[\p{L}\p{N}_-]+)*$/u;
  if (!value.startsWith("[")) {
    const items = value.split(",").map((item) => item.trim());
    return items.length > 1 && items.every((item) => words.test(item))
      ? `[${items.map(yamlValue).join(", ")}]`
      : null;
  }
  if (!value.endsWith("]")) return null;
  const inner = value.slice(1, -1);
  if (!inner.trim()) return undefined;
  const items = tagItems(inner);
  if (!items) return null;
  let changed = false;
  const repaired: string[] = [];
  for (const [index, item] of items.entries()) {
    // YAML permits a trailing comma in a flow list.
    if (!item.trim() && index === items.length - 1 && index > 0) {
      repaired.push(item);
      continue;
    }
    if (scalar(item) !== null) repaired.push(item);
    else {
      const bare = item.trim();
      const parts = bare.split(": ");
      if (parts.length !== 2 || !parts.every((part) => words.test(part))) return null;
      repaired.push(item.replace(bare, quoteYaml(bare)));
      changed = true;
    }
  }
  return changed ? `[${repaired.join(",")}]` : undefined;
}

export type TagProblem = { line: number; replacement: string | null };

/** Problems only in a closed leading block. Duplicate keys and unsupported structures stay raw. */
export function tagProblems(line: (n: number) => string, lines: number): TagProblem[] {
  const bare = (n: number) => (n === 1 ? line(n).replace(/^\uFEFF/, "") : line(n));
  const meta = frontmatter(bare, lines);
  if (!meta) return [];
  const keys: number[] = [];
  for (let n = 2; n < meta.end; n++) if (/^tags?:/.test(bare(n))) keys.push(n);
  if (keys.length > 1) return keys.map((line) => ({ line, replacement: null }));
  const problems: TagProblem[] = [];
  let inTags = false;
  for (let n = 2; n < meta.end; n++) {
    const text = bare(n);
    const key = text.match(/^(tags?:[ \t]*)(.*?)([ \t]*)$/);
    if (key) {
      inTags = !key[2];
      if (key[2]) {
        let continuation = n + 1;
        while (continuation < meta.end && /^\s*(?:#.*)?$/.test(bare(continuation))) continuation++;
        if (continuation < meta.end && /^\s/.test(bare(continuation))) {
          problems.push({ line: n, replacement: null });
          continue;
        }
      }
      const repaired = tagRepair(key[2]);
      if (repaired !== undefined)
        problems.push({
          line: n,
          replacement: repaired === null ? null : key[1] + repaired + key[3],
        });
    } else if (text.trim() && !/^\s*#/.test(text)) {
      if (!/^\s/.test(text)) inTags = false;
      else if (inTags) {
        const item = text.match(/^\s+- (.*)$/);
        if (!item || scalar(item[1]) === null) problems.push({ line: n, replacement: null });
      }
    }
  }
  return problems;
}

/** Repair only certain tag intent, preserving every other byte, including BOM and mixed endings. */
export function repairTags(text: string): string {
  const lines = text.split(/(?<=\r\n|\n|\r(?!\n))/);
  const bare = (n: number) => lines[n - 1].replace(/\r?\n$|\r$/, "");
  for (const problem of tagProblems(bare, lines.length)) {
    if (problem.replacement === null) continue;
    const ending = lines[problem.line - 1].slice(bare(problem.line).length);
    lines[problem.line - 1] = problem.replacement + ending;
  }
  return lines.join("");
}

/** Only absolute HTTP(S) metadata offers a page action. Rust validates again. */
export function pageUrl(value: string | null): string | null {
  if (!value || !/^https?:\/\//i.test(value)) return null;
  try {
    const url = new URL(value);
    return url.protocol === "https:" || url.protocol === "http:" ? value : null;
  } catch {
    return null;
  }
}

export type Frontmatter = { end: number; title: string | null; tags: string[]; url: string | null };

/**
 * A `---` block closed on a later `---` line, at the very start. Reads title, url and tag scalar and
 * list forms; unsupported YAML is ignored. `end` is the 1-based number of the closing line.
 */
export function frontmatter(line: (n: number) => string, lines: number): Frontmatter | null {
  if (lines < 2 || line(1) !== "---") return null;
  let title: string | null = null,
    url: string | null = null,
    tags: string[] = [],
    inTags = false;
  for (let n = 2; n <= lines; n++) {
    const text = line(n);
    if (text === "---") return { end: n, title, tags, url };
    if (/^\s/.test(text)) {
      const item = text.trim().match(/^- (.*)/);
      if (inTags && item) {
        const value = scalar(item[1]);
        if (value) tags.push(value);
      }
      continue;
    }
    inTags = false;
    const match = text.match(/^(title|url|tags?):\s*(.*)$/);
    if (!match) continue;
    if (match[1] === "title") title = scalar(match[2]);
    else if (match[1] === "url") url = pageUrl(scalar(match[2]));
    else {
      inTags = !match[2];
      const list = match[2].match(/^\[(.*)\]$/);
      // An unpaired quote, as in `[don't, music]`, reads as before: split at every comma.
      tags = (list ? (tagItems(list[1]) ?? list[1].split(",")) : [match[2]])
        .map(scalar)
        .filter((v): v is string => v !== null);
    }
  }
  return null;
}
