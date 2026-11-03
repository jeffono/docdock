import { syntaxTree } from "@codemirror/language";
import { type EditorState, Facet, StateField } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  WidgetType,
} from "@codemirror/view";
export const notePath = Facet.define<string, string>({ combine: (values) => values[0] ?? "" });

/** Same decoding, traversal and segment encoding rules as markdown::image_url. */
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
function active(state: EditorState, from: number, to: number) {
  return state.selection.ranges.some(
    (r) => state.doc.lineAt(r.from).from <= to && state.doc.lineAt(r.to).to >= from,
  );
}
class TextWidget extends WidgetType {
  constructor(
    readonly text: string,
    readonly cls = "",
  ) {
    super();
  }
  eq(other: TextWidget) {
    return other.text === this.text && other.cls === this.cls;
  }
  toDOM() {
    const el = document.createElement("span");
    el.textContent = this.text;
    el.className = this.cls;
    return el;
  }
}
class TaskWidget extends WidgetType {
  constructor(readonly checked: boolean) {
    super();
  }
  eq(other: TaskWidget) {
    return other.checked === this.checked;
  }
  toDOM(view: EditorView) {
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = this.checked;
    input.setAttribute("aria-label", this.checked ? "[x]" : "[ ]");
    input.onmousedown = (event) => event.preventDefault();
    input.onclick = (event) => {
      event.preventDefault();
      if (view.state.readOnly) return;
      const from = view.posAtDOM(input);
      view.dispatch({
        changes: { from: from + 1, to: from + 2, insert: this.checked ? " " : "x" },
        userEvent: "input",
      });
    };
    return input;
  }
  ignoreEvent() {
    return true;
  }
}
class ImageWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly alt: string,
  ) {
    super();
  }
  eq(other: ImageWidget) {
    return other.src === this.src && other.alt === this.alt;
  }
  toDOM() {
    const el = document.createElement("img");
    el.src = this.src;
    el.alt = this.alt;
    el.className = "preview-image";
    return el;
  }
}
function scalar(raw: string): string | null {
  const value = raw.trim();
  if (/^["']/.test(value)) {
    const end = value.indexOf(value[0], 1),
      rest = value.slice(end + 1).trim(),
      result = value.slice(1, end);
    return end > 0 && (!rest || rest.startsWith("#")) && result && !/["'\\]/.test(result)
      ? result
      : null;
  }
  const result = value.replace(/(^|\s)#.*$/, "").trim();
  return !result || /^[[{|>\-&*!%@`]/.test(result) || /["'[\]{},]|: /.test(result) ? null : result;
}
const metadataCache = new WeakMap<EditorState["doc"], ReturnType<typeof readMetadata>>();
function metadata(state: EditorState) {
  if (!metadataCache.has(state.doc)) metadataCache.set(state.doc, readMetadata(state));
  return metadataCache.get(state.doc) ?? null;
}
function readMetadata(state: EditorState) {
  if (state.doc.line(1).text !== "---") return null;
  let title: string | null = null,
    tags: string[] = [],
    inTags = false;
  for (let n = 2; n <= state.doc.lines; n++) {
    const line = state.doc.line(n);
    if (line.text === "---") return { to: line.to, title, tags };
    if (/^\s/.test(line.text)) {
      const item = line.text.trim().match(/^- (.*)/);
      if (inTags && item) {
        const value = scalar(item[1]);
        if (value) tags.push(value);
      }
      continue;
    }
    inTags = false;
    const match = line.text.match(/^(title|tags?):\s*(.*)$/);
    if (!match) continue;
    if (match[1] === "title") title = scalar(match[2]);
    else {
      tags = [];
      inTags = !match[2];
      const list = match[2].match(/^\[(.*)\]$/);
      tags = (list ? (list[1].match(/(?:"[^"]*"|'[^']*'|[^,])+/g) ?? []) : [match[2]])
        .map(scalar)
        .filter((v): v is string => v !== null);
    }
  }
  return null;
}
class HeaderWidget extends WidgetType {
  constructor(
    readonly title: string | null,
    readonly tags: string[],
  ) {
    super();
  }
  eq(other: HeaderWidget) {
    return other.title === this.title && other.tags.join("\0") === this.tags.join("\0");
  }
  toDOM(view: EditorView) {
    const header = document.createElement("header");
    header.className = "note-header";
    header.tabIndex = 0;
    const enter = () => {
      view.dispatch({
        selection: { anchor: Math.min(4, view.state.doc.length) },
        scrollIntoView: true,
      });
      view.focus();
    };
    header.onclick = enter;
    header.onkeydown = (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        enter();
      }
    };
    if (this.title) {
      const title = document.createElement("h1");
      title.textContent = this.title;
      header.append(title);
    }
    if (this.tags.length) {
      const tags = document.createElement("div");
      tags.className = "tags";
      for (const tag of this.tags) {
        const el = document.createElement("span");
        el.textContent = tag;
        tags.append(el);
      }
      header.append(tags);
    }
    if (!header.childNodes.length) header.textContent = "---";
    return header;
  }
  ignoreEvent() {
    return true;
  }
}
// Block replacements must be provided directly by state, never a view plugin.
const frontmatter = StateField.define<DecorationSet>({
  create: (state) => headerDecoration(state),
  update: (value, transaction) =>
    transaction.docChanged || transaction.selection ? headerDecoration(transaction.state) : value,
  provide: (field) => EditorView.decorations.from(field),
});
function headerDecoration(state: EditorState) {
  const meta = metadata(state);
  return meta && !active(state, 0, meta.to)
    ? Decoration.set([
        Decoration.replace({ block: true, widget: new HeaderWidget(meta.title, meta.tags) }).range(
          0,
          meta.to,
        ),
      ])
    : Decoration.none;
}
function formatting(view: EditorView, note: string): DecorationSet {
  const ranges: ReturnType<Decoration["range"]>[] = [],
    lines = new Map<number, Set<string>>();
  const meta = metadata(view.state);
  const lineClass = (from: number, to: number, cls: string) => {
    for (const visible of view.visibleRanges) {
      let pos = view.state.doc.lineAt(Math.max(from, visible.from)).from;
      const end = Math.min(to, visible.to);
      while (pos <= end) {
        const line = view.state.doc.lineAt(pos);
        if (line.to >= from) {
          const set = lines.get(pos) ?? new Set();
          set.add(cls);
          lines.set(pos, set);
        }
        if (line.to === view.state.doc.length) break;
        pos = line.to + 1;
      }
    }
  };
  for (const visible of view.visibleRanges)
    syntaxTree(view.state).iterate({
      from: visible.from,
      to: visible.to,
      enter(node) {
        const { name } = node.type;
        if (meta && node.to <= meta.to) return false;
        const raw = /^(Image|TaskMarker|ListMark)$/.test(name)
            ? view.state.sliceDoc(node.from, node.to)
            : "",
          selected = active(view.state, node.from, node.to);
        const mark = (cls: string) => {
          if (node.from < node.to)
            ranges.push(Decoration.mark({ class: cls }).range(node.from, node.to));
        };
        const replace = (widget?: WidgetType) =>
          ranges.push(Decoration.replace({ widget }).range(node.from, node.to));
        if (name === "Table") {
          lineClass(node.from, node.to, "table-raw");
          return false;
        }
        if (/^(FencedCode|CodeBlock)$/.test(name)) {
          lineClass(node.from, node.to, "code-block");
          const first = view.state.doc.lineAt(node.from),
            last = view.state.doc.lineAt(node.to);
          const start =
            name === "FencedCode" && first.number < last.number
              ? view.state.doc.line(first.number + 1).from
              : first.from;
          const closed = name === "FencedCode" && /^\s*(`{3,}|~{3,})\s*$/.test(last.text);
          const end =
            closed && last.number > first.number
              ? view.state.doc.line(last.number - 1).to
              : last.to;
          if (start <= end) {
            lineClass(start, start, "frame-start");
            lineClass(end, end, "frame-end");
          }
        }
        if (/^(ATXHeading|SetextHeading)/.test(name))
          lineClass(node.from, node.to, `heading h${name.at(-1)}`);
        if (name === "StrongEmphasis") mark("strong");
        if (name === "Emphasis") mark("emphasis");
        if (name === "Strikethrough") mark("strike");
        if (name === "InlineCode") mark("inline-code");
        // Without a destination, `[text]` is plain text in the reading view (no reference definitions).
        if (name === "Link" && node.node.getChild("URL")) mark("link");
        if (name === "Blockquote") {
          let depth = 1;
          for (let parent = node.node.parent; parent; parent = parent.parent)
            if (parent.name === "Blockquote") depth++;
          lineClass(node.from, node.to, depth === 1 ? "quote" : `quote-${Math.min(depth, 3)}`);
          // The reading view's rule (pulldown-cmark): `>`, at most one space, the kind in any case
          // and nothing else on the line. Text after the marker makes a plain quote.
          const line = view.state.doc.lineAt(node.from),
            alert = view.state
              .sliceDoc(node.from, line.to)
              .match(/^> ?\[!(note|tip|important|warning|caution)\][ \t]*$/i);
          if (alert) {
            const kind = alert[1].toLowerCase();
            lineClass(node.from, node.to, `markdown-alert-${kind}`);
            if (!active(view.state, line.from, line.to))
              ranges.push(
                Decoration.replace({
                  widget: new TextWidget(kind.toUpperCase(), "alert-title"),
                }).range(node.from + alert[0].indexOf("[!"), line.to),
              );
          }
        }
        if (/^(TaskMarker|ListMark)$/.test(name)) lineClass(node.from, node.to, "list-item");
        if (selected) return;
        if (/^(HeaderMark|QuoteMark)$/.test(name)) {
          const end = view.state.sliceDoc(node.to, node.to + 1) === " " ? node.to + 1 : node.to;
          ranges.push(Decoration.replace({}).range(node.from, end));
          return false;
        }
        if (name === "Image") {
          const url = node.node.getChild("URL"),
            alt = raw.match(/^!\[([^\]]*)\]/)?.[1] ?? "";
          if (url) {
            const src = imageUrl(view.state.sliceDoc(url.from, url.to).replace(/^<|>$/g, ""), note);
            if (src) {
              // Replace each visible line separately: view plugins cannot span line breaks.
              for (const visible of view.visibleRanges) {
                let pos = Math.max(node.from, view.state.doc.lineAt(visible.from).from);
                const end = Math.min(node.to, visible.to);
                while (pos < end) {
                  const line = view.state.doc.lineAt(pos),
                    to = Math.min(line.to, node.to);
                  if (pos < to)
                    ranges.push(
                      Decoration.replace({
                        widget: pos === node.from ? new ImageWidget(src, alt) : undefined,
                      }).range(pos, to),
                    );
                  if (pos !== node.from) lineClass(line.from, line.to, "fence");
                  pos = line.to + 1;
                }
              }
              return false;
            }
          }
          return false;
        }
        if (name === "TaskMarker") {
          replace(new TaskWidget(/x/i.test(raw)));
          return false;
        }
        if (name === "ListMark") {
          const line = view.state.doc.lineAt(node.from);
          if (/^\s*[-+*] \[[ xX]\]/.test(line.text)) {
            ranges.push(Decoration.replace({}).range(node.from, node.to + 1));
            return false;
          }
          replace(new TextWidget(/^\d/.test(raw) ? raw : "•", "list-marker"));
          return false;
        }
        if (name === "HorizontalRule") {
          replace(new TextWidget("", "rule"));
          return false;
        }
        if (/^(CodeMark|CodeInfo)$/.test(name) && node.node.parent?.name === "FencedCode") {
          replace();
          lineClass(node.from, node.to, "fence");
          return false;
        }
        if (
          /^(HeaderMark|EmphasisMark|StrikethroughMark|CodeMark|LinkMark|URL|QuoteMark)$/.test(
            name,
          ) &&
          (name !== "LinkMark" || node.node.parent?.getChild("URL")) &&
          view.state.doc.lineAt(node.from).number === view.state.doc.lineAt(node.to).number
        )
          replace();
      },
    });
  if (meta && active(view.state, 0, meta.to)) {
    lineClass(0, meta.to, "frontmatter-raw");
    lineClass(0, 0, "frame-start");
    lineClass(meta.to, meta.to, "frame-end");
  }
  for (const visible of view.visibleRanges) {
    for (
      let n = view.state.doc.lineAt(visible.from).number;
      n <= view.state.doc.lineAt(visible.to).number;
      n++
    ) {
      const line = view.state.doc.line(n);
      if (!line.text && !active(view.state, line.from, line.to))
        lineClass(line.from, line.to, "blank-line");
    }
  }
  for (const [from, classes] of lines)
    ranges.push(Decoration.line({ class: [...classes].join(" ") }).range(from));
  // Overlapping visible ranges can visit the same syntax node; keep decorations unique.
  const unique = new Map(
    ranges.map((r) => [`${r.from}:${r.to}:${JSON.stringify(r.value.spec)}`, r]),
  );
  return Decoration.set([...unique.values()], true);
}
export function liveFormatting() {
  return [
    frontmatter,
    ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;
        constructor(view: EditorView) {
          this.decorations = formatting(view, view.state.facet(notePath));
        }
        update(update: import("@codemirror/view").ViewUpdate) {
          if (
            update.docChanged ||
            update.selectionSet ||
            update.viewportChanged ||
            update.startState.facet(notePath) !== update.state.facet(notePath) ||
            update.startState.field(frontmatter) !== update.state.field(frontmatter) ||
            syntaxTree(update.startState) !== syntaxTree(update.state)
          )
            this.decorations = formatting(update.view, update.state.facet(notePath));
        }
      },
      { decorations: (value) => value.decorations },
    ),
  ];
}
