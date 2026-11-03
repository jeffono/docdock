// Reading view: notes rendered from the editor's own parser into a small element model, then into
// DOM nodes. Note text only ever becomes text nodes or allowlisted attribute values; no HTML is parsed.
import type { SyntaxNode } from "@lezer/common";
import {
  alertKind,
  type Definitions,
  definitions,
  footnoteLabel,
  frontmatter,
  imageUrl,
  linkTarget,
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

/** Renders a whole note: the frontmatter header, then the Markdown after it. */
export function noteModel(source: string, note: string): Content[] {
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

class Renderer {
  constructor(
    readonly slice: (from: number, to: number) => string,
    readonly note: string,
    readonly defs: Definitions,
  ) {}

  blocks(parent: SyntaxNode, out: Content[], skipTo = -1) {
    for (let node = parent.firstChild; node; node = node.nextSibling) {
      if (node.name === "QuoteMark" || node.name === "ListMark") continue;
      const block = this.block(node, skipTo);
      if (block) out.push(block);
    }
  }

  block(node: SyntaxNode, skipTo = -1): Element | null {
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
      case "CodeBlock":
        return el("pre", {}, [el("code", {}, [this.code(node)])]);
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
    const lines = this.slice(node.from, node.to).split("\n");
    const lineStart = this.slice(0, node.from).lastIndexOf("\n") + 1;
    const indent = node.from - lineStart;
    const body = node.name === "FencedCode" ? lines.slice(1) : lines;
    if (node.name === "FencedCode" && node.lastChild?.name === "CodeMark" && lines.length > 1)
      body.pop();
    return body
      .map((line, i) =>
        node.name === "CodeBlock" && i === 0
          ? line
          : line.replace(new RegExp(`^[ \\t>]{0,${indent}}`), ""),
      )
      .join("\n");
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
        const src = imageUrl(target.url, this.note);
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

let entities: Document | null = null;
/** A named or numeric character reference, decoded by the browser in an inert document. */
function decodeEntity(entity: string): string {
  if (!/^&(#\d{1,7}|#x[\da-f]{1,6}|[a-z][a-z\d]{1,31});$/i.test(entity)) return entity;
  if (typeof DOMParser === "undefined") return entity;
  entities ??= new DOMParser().parseFromString("", "text/html");
  const holder = entities.createElement("p");
  // Safe: the string matched a single character reference, with no markup characters.
  holder.innerHTML = entity;
  return holder.textContent || entity;
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

export function renderNote(text: string, note: string): DocumentFragment {
  return toDom(noteModel(text, note));
}
