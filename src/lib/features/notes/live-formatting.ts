import { syntaxTree } from "@codemirror/language";
import { type EditorState, Facet, StateField } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  WidgetType,
} from "@codemirror/view";
import { IterMode } from "@lezer/common";
import {
  alertKind,
  definitions,
  footnoteLabel,
  imageUrl,
  linkTarget,
  frontmatter as readFrontmatter,
  safeHref,
} from "./markdown";
export const notePath = Facet.define<string, string>({ combine: (values) => values[0] ?? "" });

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
type Metadata = { to: number; title: string | null; tags: string[] } | null;
const metadataCache = new WeakMap<EditorState["doc"], Metadata>();
function metadata(state: EditorState): Metadata {
  const { doc } = state;
  if (!metadataCache.has(doc)) {
    const meta = readFrontmatter((n) => doc.line(n).text, doc.lines);
    metadataCache.set(
      doc,
      meta && { to: doc.line(meta.end).to, title: meta.title, tags: meta.tags },
    );
  }
  return metadataCache.get(doc) ?? null;
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
  const slice = (from: number, to: number) => view.state.sliceDoc(from, to);
  const sameLine = (from: number, to: number) =>
    view.state.doc.lineAt(from).number === view.state.doc.lineAt(to).number;
  const defs = definitions(syntaxTree(view.state).topNode, slice);
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
      mode: IterMode.IgnoreMounts,
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
        if (name === "Link" || name === "Autolink") {
          // Without a definition, `[text]` stays text, as in the reading view.
          const target =
            name === "Link" ? linkTarget(node.node, slice, defs)?.url : slice(node.from, node.to);
          if (!target) return;
          if (safeHref(target.replace(/^<|>$/g, ""))) mark("link");
          const marks = node.node.getChildren("LinkMark");
          if (!selected && marks.length >= 2 && sameLine(node.from, node.to)) {
            // Hide `[`, and everything from `]` on: destination, title or reference label.
            ranges.push(Decoration.replace({}).range(node.from, marks[0].to));
            ranges.push(Decoration.replace({}).range(marks[1].from, node.to));
          }
          return name === "Link" ? undefined : false;
        }
        if (
          name === "URL" &&
          node.node.parent?.name !== "Link" &&
          safeHref(slice(node.from, node.to))
        )
          mark("link");
        if (name === "FootnoteReference") {
          const label = footnoteLabel(slice(node.from, node.to));
          if (defs.footnotes.has(label) && !selected)
            replace(new TextWidget(label, "footnote-ref"));
          return false;
        }
        if (name === "FootnoteDefinition") {
          lineClass(node.from, node.to, "footnote");
          const footnote = node.node.getChild("FootnoteMark");
          if (footnote && !selected)
            ranges.push(
              Decoration.replace({
                widget: new TextWidget(
                  footnoteLabel(slice(footnote.from, footnote.to)),
                  "footnote-label",
                ),
              }).range(footnote.from, footnote.to),
            );
        }
        if (name === "Blockquote") {
          let depth = 1;
          for (let parent = node.node.parent; parent; parent = parent.parent)
            if (parent.name === "Blockquote") depth++;
          lineClass(node.from, node.to, depth === 1 ? "quote" : `quote-${Math.min(depth, 3)}`);
          const line = view.state.doc.lineAt(node.from),
            first = slice(node.from, line.to),
            kind = alertKind(first);
          if (kind) {
            lineClass(node.from, node.to, `markdown-alert-${kind}`);
            if (!active(view.state, line.from, line.to))
              ranges.push(
                Decoration.replace({
                  widget: new TextWidget(kind.toUpperCase(), "alert-title"),
                }).range(node.from + first.indexOf("[!"), line.to),
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
          const url = linkTarget(node.node, slice, defs)?.url,
            alt = raw.match(/^!\[([^\]]*)\]/)?.[1] ?? "";
          if (url) {
            const src = imageUrl(url, note);
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
          /^(HeaderMark|EmphasisMark|StrikethroughMark|CodeMark|QuoteMark)$/.test(name) &&
          sameLine(node.from, node.to)
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
