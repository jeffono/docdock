// Reading view: notes rendered from the editor's own parser into a small element model, then into
// DOM nodes. Note text only ever becomes text nodes or allowlisted attribute values; no HTML is parsed.
import type { SyntaxNode } from "@lezer/common";
import { codeLanguage, codeSpans } from "./code-colors";
import {
  alertKind,
  type Definitions,
  decodeEntity,
  definitions,
  footnoteLabel,
  frontmatter,
  imageUrl,
  linkTarget,
  maxDepth,
  noteParser,
  safeHref,
} from "./markdown";

type Tag =
  | "a"
  | "blockquote"
  | "br"
  | "code"
  | "del"
  | "div"
  | "em"
  | "h1"
  | "h2"
  | "h3"
  | "h4"
  | "h5"
  | "h6"
  | "header"
  | "hr"
  | "img"
  | "input"
  | "li"
  | "ol"
  | "p"
  | "pre"
  | "span"
  | "strong"
  | "table"
  | "tbody"
  | "td"
  | "th"
  | "thead"
  | "tr"
  | "ul";
type Attrs = {
  class?: string;
  href?: string;
  title?: string;
  src?: string;
  alt?: string;
  id?: string;
  start?: number;
  checked?: boolean;
};
export type Element = { tag: Tag; attrs?: Attrs; children?: Content[] };
export type Content = Element | string;

const el = (tag: Tag, attrs?: Attrs, children?: Content[]): Element => ({ tag, attrs, children });

/** Renders a whole note: the frontmatter header, then the Markdown after it. `note` is its path. */
export function noteModel(source: string, note: string | null): Content[] {
  const text = source.replace(/\r\n?/g, "\n");
  const lines = text.split("\n");
  const meta = frontmatter((n) => lines[n - 1], lines.length);
  const out: Content[] = [];
  let body = text;
  if (meta) {
    body = lines.slice(meta.end).join("\n");
    if (meta.title || meta.tags.length) {
      const header: Content[] = [];
      if (meta.title) header.push(el("h1", {}, [meta.title]));
      if (meta.tags.length)
        header.push(
          el(
            "div",
            { class: "tags" },
            meta.tags.map((tag) => el("span", {}, [tag])),
          ),
        );
      out.push(el("header", { class: "note-header" }, header));
    }
  }
  const tree = noteParser.parse(body);
  const slice = (from: number, to: number) => body.slice(from, to);
  const renderer = new Renderer(slice, note, definitions(tree.topNode, slice));
  renderer.blocks(tree.topNode, out);
  return out;
}

const blockTags = new Set<Tag>([
  "blockquote",
  "br",
  "div",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "header",
  "li",
  "p",
  "pre",
  "td",
  "th",
]);
/** The text a reader sees, with blocks kept apart by spaces. */
function plain(content: Content): string {
  if (typeof content === "string") return content;
  const inner = (content.children ?? []).map(plain).join("");
  return blockTags.has(content.tag) ? ` ${inner} ` : inner;
}
const words = (content: Content) => plain(content).replace(/\s+/g, " ").trim();
/** Names are cut at a word near this length. */
const NAME_LENGTH = 60;

/**
 * The name for a note still called `Untitled`, as a reader sees it: the frontmatter title (marked, as
 * the file then follows a title that stays), else a top-level first heading, else the first words of the
 * text. Null when the note shows no text.
 */
export function noteName(source: string): { title: string; frontmatter: boolean } | null {
  const model = noteModel(source, "");
  const meta = model[0];
  const metaTitle =
    typeof meta !== "string" && meta?.tag === "header"
      ? meta.children?.find((child) => typeof child !== "string" && child.tag === "h1")
      : undefined;
  if (metaTitle && words(metaTitle)) return { title: words(metaTitle), frontmatter: true };
  const body = model.filter((block) => typeof block === "string" || block.tag !== "header");
  const top = body.find((block) => words(block));
  if (!top) return null;
  if (typeof top !== "string" && top.tag === "h1") return { title: words(top), frontmatter: false };
  const text = Array.from(body.map(words).join(" ").replace(/\s+/g, " ").trim());
  if (text.length <= NAME_LENGTH) return { title: text.join(""), frontmatter: false };
  // Cut in code points: a lone half of a surrogate pair is no valid file name.
  const cut = text.slice(0, NAME_LENGTH + 1);
  const space = cut.lastIndexOf(" ");
  return { title: cut.slice(0, space > 0 ? space : NAME_LENGTH).join(""), frontmatter: false };
}

class Renderer {
  constructor(
    readonly slice: (from: number, to: number) => string,
    /** Null for content that is not a note, such as a page article: it never shows local images. */
    readonly note: string | null,
    readonly defs: Definitions,
  ) {}

  /** Current nesting; beyond `maxDepth` the rest of a node is shown as text. */
  depth = 0;
  nested<T>(node: SyntaxNode, render: () => T, fallback: (text: string) => T): T {
    if (this.depth >= maxDepth) return fallback(this.slice(node.from, node.to));
    this.depth++;
    try {
      return render();
    } finally {
      this.depth--;
    }
  }

  blocks(parent: SyntaxNode, out: Content[], skipTo = -1) {
    for (let node = parent.firstChild; node; node = node.nextSibling) {
      if (node.name === "QuoteMark" || node.name === "ListMark") continue;
      const block = this.block(node, skipTo);
      if (block) out.push(block);
    }
  }

  block(node: SyntaxNode, skipTo = -1): Element | null {
    return this.nested(
      node,
      () => this.blockAt(node, skipTo),
      (text) => el("p", { class: "raw" }, [text]),
    );
  }

  blockAt(node: SyntaxNode, skipTo: number): Element | null {
    const { name } = node;
    const heading = name.match(/^(?:ATX|Setext)Heading(\d)$/);
    if (heading) return el(`h${heading[1]}` as Tag, {}, this.inline(node, node.from, node.to));
    switch (name) {
      case "Paragraph": {
        const from = Math.max(node.from, skipTo);
        const children = this.inline(node, from, node.to);
        return children.length ? el("p", {}, children) : null;
      }
      case "Blockquote": {
        const line = this.slice(node.from, node.to).split("\n")[0];
        const kind = alertKind(line);
        const children: Content[] = [];
        this.blocks(node, children, kind ? node.from + line.length + 1 : -1);
        return el("blockquote", kind ? { class: `markdown-alert-${kind}` } : {}, children);
      }
      case "BulletList":
      case "OrderedList": {
        const items: Content[] = [];
        this.blocks(node, items);
        const first = node.firstChild?.getChild("ListMark");
        const start = first ? Number.parseInt(this.slice(first.from, first.to), 10) : 1;
        return name === "BulletList"
          ? el("ul", {}, items)
          : el("ol", start !== 1 && Number.isFinite(start) ? { start } : {}, items);
      }
      case "ListItem": {
        const children: Content[] = [];
        for (let child = node.firstChild; child; child = child.nextSibling) {
          if (child.name === "ListMark") continue;
          if (child.name === "Task") {
            const marker = child.getChild("TaskMarker");
            const checked = !!marker && /x/i.test(this.slice(marker.from, marker.to));
            children.push(
              el("input", { checked }),
              ...this.inline(child, marker?.to ?? child.from, child.to),
            );
          } else {
            const block = this.block(child);
            if (block) children.push(block);
          }
        }
        return el("li", {}, children);
      }
      case "FencedCode":
      case "CodeBlock": {
        const text = this.code(node);
        const info = node.getChild("CodeInfo");
        const children: Content[] = [];
        let pos = 0;
        for (const span of codeSpans(text, info ? this.slice(info.from, info.to) : "")) {
          if (span.from > pos) children.push(text.slice(pos, span.from));
          children.push(el("span", { class: span.className }, [text.slice(span.from, span.to)]));
          pos = span.to;
        }
        if (pos < text.length) children.push(text.slice(pos));
        return el("pre", {}, [el("code", {}, children)]);
      }
      case "HorizontalRule":
        return el("hr");
      case "Table":
        return this.table(node);
      case "LinkReference":
        return null;
      case "FootnoteDefinition": {
        const mark = node.getChild("FootnoteMark");
        const label = mark ? footnoteLabel(this.slice(mark.from, mark.to)) : "";
        return el("div", { class: "footnote", id: `note-fn-${label}` }, [
          el("span", { class: "footnote-label" }, [label]),
          " ",
          ...this.inline(node, mark?.to ?? node.from, node.to),
        ]);
      }
      default:
        // Raw HTML blocks, comments and anything unknown stay visible as text.
        return el("p", { class: "raw" }, [this.slice(node.from, node.to)]);
    }
  }

  /** Code lines without their fences and without the container prefix the fence itself has. */
  code(node: SyntaxNode): string {
    // Container prefixes: the quote markers the parser assigns to the block, then as many spaces
    // as precede the code's own start (list or quote indentation).
    let indent = 0;
    while (node.from - indent > 0 && this.slice(node.from - indent - 1, node.from - indent) === " ")
      indent++;
    // The one space after a quote marker belongs to the marker.
    if (indent && this.slice(node.from - indent - 1, node.from - indent) === ">") indent--;
    const quotes = node.getChildren("QuoteMark");
    const lines: string[] = [];
    let start = node.from;
    for (const line of this.slice(node.from, node.to).split("\n")) {
      const end = start + line.length;
      if (start === node.from) lines.push(line);
      else {
        let from = start;
        for (const quote of quotes)
          if (quote.from >= start && quote.to <= end) from = Math.max(from, quote.to);
        if (from > start && this.slice(from, from + 1) === " ") from++;
        lines.push(this.slice(from, end).replace(new RegExp(`^ {0,${indent}}`), ""));
      }
      start = end + 1;
    }
    if (node.name === "FencedCode") {
      lines.shift();
      if (node.lastChild?.name === "CodeMark" && lines.length) lines.pop();
    }
    return lines.join("\n");
  }

  table(node: SyntaxNode): Element {
    const rows = node.getChildren("TableRow");
    const header = node.getChild("TableHeader");
    const delimiter = node.getChildren("TableDelimiter")[0];
    const aligns = delimiter
      ? this.slice(delimiter.from, delimiter.to)
          .replace(/^\||\|$/g, "")
          .split("|")
          .map((cell) => {
            const value = cell.trim();
            if (value.startsWith(":") && value.endsWith(":")) return "align-center";
            if (value.endsWith(":")) return "align-right";
            return "";
          })
      : [];
    const row = (line: SyntaxNode, tag: "th" | "td") =>
      el(
        "tr",
        {},
        line
          .getChildren("TableCell")
          .map((cell, i) =>
            el(tag, aligns[i] ? { class: aligns[i] } : {}, this.inline(cell, cell.from, cell.to)),
          ),
      );
    const parts: Content[] = [];
    if (header) parts.push(el("thead", {}, [row(header, "th")]));
    if (rows.length)
      parts.push(
        el(
          "tbody",
          {},
          rows.map((line) => row(line, "td")),
        ),
      );
    return el("table", {}, parts);
  }

  /** Inline content of `parent` between `from` and `to`; syntax marks are dropped. */
  inline(parent: SyntaxNode, from: number, to: number): Content[] {
    const out: Content[] = [];
    let pos = from;
    const text = (end: number) => {
      // Continuation-line indentation is not content.
      if (end > pos) push(out, this.slice(pos, end).replace(/\n[ \t]+/g, "\n"));
    };
    for (let child = parent.firstChild; child; child = child.nextSibling) {
      if (child.to <= from) continue;
      if (child.from >= to) break;
      text(child.from);
      pos = Math.max(pos, child.to);
      // Continuation-line prefixes and the space after them are container syntax.
      if (child.name === "QuoteMark" && this.slice(pos, pos + 1) === " ") pos++;
      for (const item of this.inlineNode(child)) push(out, item);
    }
    text(to);
    // Leading and trailing whitespace of a block is not content.
    if (typeof out[0] === "string") out[0] = out[0].trimStart();
    const last = out.length - 1;
    if (typeof out[last] === "string") out[last] = (out[last] as string).trimEnd();
    return out.filter((item) => item !== "");
  }

  inlineNode(node: SyntaxNode): Content[] {
    return this.nested(
      node,
      () => this.inlineAt(node),
      (text) => [text],
    );
  }

  inlineAt(node: SyntaxNode): Content[] {
    const all = () => this.inline(node, node.from, node.to);
    switch (node.name) {
      case "Emphasis":
        return [el("em", {}, all())];
      case "StrongEmphasis":
        return [el("strong", {}, all())];
      case "Strikethrough":
        return [el("del", {}, all())];
      case "InlineCode": {
        const marks = node.getChildren("CodeMark");
        const code = this.slice(marks[0]?.to ?? node.from, marks[1]?.from ?? node.to)
          .replace(/\n/g, " ")
          .replace(/^ (.*[^ ].*) $/, "$1");
        return [el("code", {}, [code])];
      }
      case "Link": {
        const marks = node.getChildren("LinkMark");
        const label = this.inline(node, marks[0]?.to ?? node.from, marks[1]?.from ?? node.to);
        const target = linkTarget(node, this.slice, this.defs);
        if (!target) return [this.slice(node.from, node.to)];
        const href = safeHref(target.url);
        return href
          ? [el("a", target.title ? { href, title: target.title } : { href }, label)]
          : label;
      }
      case "Image": {
        const marks = node.getChildren("LinkMark");
        const alt = this.plain(
          this.inline(node, marks[0]?.to ?? node.from, marks[1]?.from ?? node.to),
        );
        const target = linkTarget(node, this.slice, this.defs);
        if (!target) return [this.slice(node.from, node.to)];
        const src = this.note === null ? null : imageUrl(target.url, this.note);
        return src
          ? [el("img", target.title ? { src, alt, title: target.title } : { src, alt })]
          : [alt];
      }
      case "Autolink":
      case "URL": {
        const url = node.name === "URL" ? node : node.getChild("URL");
        const value = url ? this.slice(url.from, url.to) : this.slice(node.from, node.to);
        const href = safeHref(value);
        return href ? [el("a", { href }, [value])] : [this.slice(node.from, node.to)];
      }
      case "FootnoteReference": {
        const label = footnoteLabel(this.slice(node.from, node.to));
        return this.defs.footnotes.has(label)
          ? [el("a", { class: "footnote-ref", href: `#fn-${label}` }, [label])]
          : [this.slice(node.from, node.to)];
      }
      case "Escape":
        return [this.slice(node.from + 1, node.to)];
      case "Entity":
        return [decodeEntity(this.slice(node.from, node.to))];
      case "HardBreak":
        return [el("br")];
      case "EmphasisMark":
      case "StrikethroughMark":
      case "HeaderMark":
      case "QuoteMark":
      case "LinkMark":
      case "CodeMark":
      case "TaskMarker":
      case "FootnoteMark":
        return [];
      default:
        // Inline HTML, comments and anything unknown stay visible as text.
        return [this.slice(node.from, node.to)];
    }
  }

  plain(content: Content[]): string {
    return content
      .map((item) => (typeof item === "string" ? item : this.plain(item.children ?? [])))
      .join("");
  }
}

function push(out: Content[], item: Content) {
  const last = out.length - 1;
  if (typeof item === "string" && typeof out[last] === "string") out[last] += item;
  else out.push(item);
}

/** Builds DOM nodes from the model; only the tags and attributes of the model types exist. */
export function toDom(content: Content[], doc: Document = document): DocumentFragment {
  const fragment = doc.createDocumentFragment();
  for (const item of content) {
    if (typeof item === "string") {
      fragment.append(doc.createTextNode(item));
      continue;
    }
    const node = doc.createElement(item.tag);
    const attrs = item.attrs ?? {};
    if (attrs.class) node.className = attrs.class;
    if (attrs.id) node.id = attrs.id;
    if (attrs.href) node.setAttribute("href", attrs.href);
    if (attrs.title) node.title = attrs.title;
    if (attrs.src) node.setAttribute("src", attrs.src);
    if (attrs.alt !== undefined) node.setAttribute("alt", attrs.alt);
    if (attrs.start !== undefined) node.setAttribute("start", String(attrs.start));
    if (item.tag === "input") {
      const input = node as HTMLInputElement;
      input.type = "checkbox";
      input.disabled = true;
      input.checked = !!attrs.checked;
    }
    if (item.children) node.append(toDom(item.children, doc));
    fragment.append(node);
  }
  return fragment;
}

/** A note that cannot be rendered still shows its text. */
export async function renderNote(text: string, note: string | null): Promise<DocumentFragment> {
  try {
    const loads: Promise<unknown>[] = [];
    noteParser.parse(text).iterate({
      enter(node) {
        if (node.name !== "FencedCode") return;
        const info = node.node.getChild("CodeInfo");
        const language = info && codeLanguage(text.slice(info.from, info.to));
        if (language) loads.push(language.load());
      },
    });
    // A failed chunk must not hide the note, including other successfully loaded grammars.
    await Promise.allSettled(loads);
    return toDom(noteModel(text, note));
  } catch (error) {
    console.error(error);
    return toDom([el("p", { class: "raw" }, [text])]);
  }
}
