import { syntaxTree } from "@codemirror/language";
import { type EditorState, Facet, Prec, StateField } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  keymap,
  ViewPlugin,
  WidgetType,
} from "@codemirror/view";
import { IterMode, type SyntaxNode } from "@lezer/common";
import {
  alertKind,
  definitions,
  footnoteLabel,
  imageRuns,
  imageUrl,
  linkTarget,
  frontmatter as readFrontmatter,
  safeHref,
} from "./markdown";
import { pointerReveal, revealChanged, revealed } from "./pointer-reveal";
export const notePath = Facet.define<string, string>({ combine: (values) => values[0] ?? "" });

function active(state: EditorState, from: number, to: number) {
  return revealed(state).some(
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
/**
 * A small copy of the image beside its Markdown while the cursor is on it, so it reads as which. It
 * stands in for the Markdown's "!", which it shows after it, so the cursor takes both as one step and
 * a cursor at the image's start is before the copy.
 */
class ImageChipWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly alt: string,
  ) {
    super();
  }
  eq(other: ImageChipWidget) {
    return other.src === this.src && other.alt === this.alt;
  }
  toDOM() {
    const chip = document.createElement("span");
    const el = document.createElement("img");
    el.src = this.src;
    el.alt = this.alt;
    el.className = "image-chip";
    el.draggable = false;
    chip.append(el, "!");
    return chip;
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
/** An image of a grid: what it shows, and where its Markdown starts from the grid's start. */
type GridImage = { src: string; alt: string; offset: number };
/**
 * Lines of two or more images shown as the reading view shows them, a grid. A click on an image
 * puts the cursor on its Markdown, which shows the lines again.
 */
class GridWidget extends WidgetType {
  constructor(readonly images: readonly GridImage[]) {
    super();
  }
  eq(other: GridWidget) {
    return (
      other.images.length === this.images.length &&
      other.images.every(
        (image, i) =>
          image.src === this.images[i].src &&
          image.alt === this.images[i].alt &&
          image.offset === this.images[i].offset,
      )
    );
  }
  toDOM(view: EditorView) {
    const grid = document.createElement("div");
    grid.className = "image-grid";
    for (const image of this.images) {
      const thumb = document.createElement("button");
      thumb.type = "button";
      thumb.tabIndex = -1;
      thumb.className = "image-thumb";
      thumb.setAttribute(
        "aria-label",
        image.alt || image.src.slice(image.src.lastIndexOf("/") + 1),
      );
      const img = document.createElement("img");
      img.src = image.src;
      img.alt = image.alt;
      img.draggable = false;
      thumb.append(img);
      thumb.addEventListener("mousedown", (event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        // From where the grid is now, which edits above it may have moved.
        const at = view.posAtDOM(grid) + image.offset;
        view.dispatch({ selection: { anchor: at }, scrollIntoView: true });
        view.focus();
      });
      grid.append(thumb);
    }
    return grid;
  }
  ignoreEvent() {
    return true;
  }
}
/** A grid in the editor: the whole lines of a run of local images (`imageRuns`), and its images. */
type EditorGrid = { from: number; to: number; images: GridImage[] };
/**
 * The grids of a paragraph of whole lines, outside lists and quotes, since only whole lines can be
 * replaced as a block: its runs of image lines whose images are all local.
 */
function paragraphGrids(
  state: EditorState,
  node: SyntaxNode,
  slice: (from: number, to: number) => string,
  defs: ReturnType<typeof definitions>,
  note: string,
): EditorGrid[] {
  const { doc } = state;
  if (doc.lineAt(node.from).from !== node.from) return [];
  const grids: EditorGrid[] = [];
  runs: for (const run of imageRuns(node, slice)) {
    const from = doc.lineAt(run.from).from;
    const images: GridImage[] = [];
    for (const child of run.images) {
      const url = linkTarget(child, slice, defs)?.url;
      const src = url && imageUrl(url, note);
      if (!src) continue runs;
      const alt = slice(child.from, child.to).match(/^!\[([^\]]*)\]/)?.[1] ?? "";
      images.push({ src, alt, offset: child.from - from });
    }
    grids.push({ from, to: doc.lineAt(run.to).to, images });
  }
  return grids;
}
/**
 * ↑ and ↓ would jump over a grid, which is one block to the cursor: they enter it instead, at its
 * first line from above and its last from below, which shows its lines.
 */
const gridKeys = Prec.high(
  keymap.of([
    {
      key: "ArrowDown",
      run: (view) => {
        const { state } = view;
        const { main } = state.selection;
        if (!main.empty || state.selection.ranges.length > 1) return false;
        const line = state.doc.lineAt(main.head);
        if (line.to >= state.doc.length || !gridAt(state, line.to + 1)) return false;
        view.dispatch({ selection: { anchor: line.to + 1 }, scrollIntoView: true });
        return true;
      },
    },
    {
      key: "ArrowUp",
      run: (view) => {
        const { state } = view;
        const { main } = state.selection;
        if (!main.empty || state.selection.ranges.length > 1) return false;
        const line = state.doc.lineAt(main.head);
        if (line.from === 0) return false;
        let start = -1;
        state.field(grids).between(line.from - 1, line.from - 1, (_from, to) => {
          if (to === line.from - 1) start = state.doc.lineAt(to).from;
        });
        if (start < 0) return false;
        view.dispatch({ selection: { anchor: start }, scrollIntoView: true });
        return true;
      },
    },
  ]),
);
// A grid spans lines, so it is a block replacement, which only state may provide.
export const grids = StateField.define<DecorationSet>({
  create: (state) => gridDecorations(state),
  update: (value, transaction) =>
    transaction.docChanged ||
    revealChanged(transaction) ||
    syntaxTree(transaction.startState) !== syntaxTree(transaction.state) ||
    transaction.startState.facet(notePath) !== transaction.state.facet(notePath)
      ? gridDecorations(transaction.state)
      : value,
  provide: (field) => EditorView.decorations.from(field),
});
function gridDecorations(state: EditorState): DecorationSet {
  // Cheap way out for the common note without any image syntax.
  if (!state.doc.sliceString(0).includes("![")) return Decoration.none;
  const slice = (from: number, to: number) => state.sliceDoc(from, to);
  const tree = syntaxTree(state);
  const defs = definitions(tree.topNode, slice);
  const note = state.facet(notePath);
  const meta = metadata(state);
  const ranges: ReturnType<Decoration["range"]>[] = [];
  tree.iterate({
    enter(node) {
      if (meta && node.to <= meta.to) return false;
      if (node.name !== "Paragraph") return;
      for (const grid of paragraphGrids(state, node.node, slice, defs, note))
        if (!active(state, grid.from, grid.to))
          ranges.push(
            Decoration.replace({ block: true, widget: new GridWidget(grid.images) }).range(
              grid.from,
              grid.to,
            ),
          );
      return false;
    },
  });
  return Decoration.set(ranges);
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
    transaction.docChanged || revealChanged(transaction)
      ? headerDecoration(transaction.state)
      : value,
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
/** Whether a grid shown now covers `pos`. */
function inGrid(state: EditorState, pos: number): boolean {
  let found = false;
  state.field(grids).between(pos, pos, (from, to) => {
    if (from <= pos && pos <= to) found = true;
  });
  return found;
}
function gridAt(state: EditorState, from: number): boolean {
  let found = false;
  state.field(grids).between(from, from, (start) => {
    if (start === from) found = true;
  });
  return found;
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
  // A grid the cursor is in shows all its lines as Markdown, not only the cursor's.
  const openGrids: [number, number][] = [];
  for (const visible of view.visibleRanges)
    syntaxTree(view.state).iterate({
      mode: IterMode.IgnoreMounts,
      from: visible.from,
      to: visible.to,
      enter(node) {
        const { name } = node.type;
        if (meta && node.to <= meta.to) return false;
        if (name === "Paragraph")
          for (const grid of paragraphGrids(view.state, node.node, slice, defs, note))
            if (active(view.state, grid.from, grid.to)) openGrids.push([grid.from, grid.to]);
        // A grid shown takes its images' place; they get no decorations of their own.
        if (name === "Image" && inGrid(view.state, node.from)) return false;
        const raw = /^(Image|TaskMarker|ListMark)$/.test(name)
            ? view.state.sliceDoc(node.from, node.to)
            : "",
          selected =
            active(view.state, node.from, node.to) ||
            openGrids.some(([from, to]) => node.from >= from && node.to <= to);
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
          const hideFences = name === "FencedCode" && !selected;
          const start =
            hideFences && first.number < last.number
              ? view.state.doc.line(first.number + 1).from
              : first.from;
          const end =
            hideFences && last.number > first.number && node.node.lastChild?.name === "CodeMark"
              ? view.state.doc.line(last.number - 1).to
              : last.to;
          // Keep a frame on the opening line when the block has no body lines.
          const frameStart = start <= end ? start : first.from;
          lineClass(frameStart, frameStart, "frame-start");
          lineClass(end, end, "frame-end");
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
        if (name === "CodeMark" || name === "CodeInfo") {
          const parent = node.node.parent;
          if (parent && !active(view.state, parent.from, parent.to)) {
            replace();
            if (parent.name === "FencedCode") lineClass(node.from, node.to, "fence");
          }
          return false;
        }
        if (selected && name === "Image") {
          const url = linkTarget(node.node, slice, defs)?.url,
            src = url && imageUrl(url, note);
          if (src)
            ranges.push(
              Decoration.replace({
                widget: new ImageChipWidget(src, raw.match(/^!\[([^\]]*)\]/)?.[1] ?? ""),
              }).range(node.from, node.from + 1),
            );
        }
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
        if (
          /^(HeaderMark|EmphasisMark|StrikethroughMark|QuoteMark)$/.test(name) &&
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
/**
 * Images and grids change size as an edit shows or hides them, and images load after the editor
 * has scrolled to the cursor: shortly after an edit, a change of height brings the cursor back into
 * view if it left it.
 */
const cursorAfterResize = ViewPlugin.fromClass(
  class {
    edited = 0;
    update(update: import("@codemirror/view").ViewUpdate) {
      if (update.transactions.some((tr) => tr.isUserEvent("input") || tr.isUserEvent("delete")))
        this.edited = Date.now();
      if (!update.heightChanged || Date.now() - this.edited > 1000 || !update.view.hasFocus) return;
      const { view } = update;
      // A view cannot dispatch during its own update.
      requestAnimationFrame(() => {
        if (view.dom.isConnected) view.dispatch({ scrollIntoView: true });
      });
    }
  },
);
export function liveFormatting() {
  return [
    cursorAfterResize,
    pointerReveal(),
    frontmatter,
    grids,
    gridKeys,
    ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;
        constructor(view: EditorView) {
          this.decorations = formatting(view, view.state.facet(notePath));
        }
        update(update: import("@codemirror/view").ViewUpdate) {
          if (
            update.docChanged ||
            revealChanged(update) ||
            update.viewportChanged ||
            update.startState.facet(notePath) !== update.state.facet(notePath) ||
            update.startState.field(frontmatter) !== update.state.field(frontmatter) ||
            update.startState.field(grids) !== update.state.field(grids) ||
            syntaxTree(update.startState) !== syntaxTree(update.state)
          )
            this.decorations = formatting(update.view, update.state.facet(notePath));
        }
      },
      { decorations: (value) => value.decorations },
    ),
  ];
}
