import { syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { SyntaxNode } from "@lezer/common";
import { systemLink } from "./link-actions";
import { definitions, linkTarget, safeHref } from "./markdown";

/** Find a Markdown link independently of whether its syntax is visible at the cursor. */
export function editorLinkAt(state: EditorState, at: number, side: -1 | 1): URL | null {
  const tree = syntaxTree(state);
  const slice = (from: number, to: number) => state.sliceDoc(from, to);
  let bare: string | undefined;
  for (let node: SyntaxNode | null = tree.resolve(at, side); node; node = node.parent) {
    if (node.name === "Image" || node.name === "LinkReference") return null;
    if (node.name === "Link") {
      const target = linkTarget(node, slice, definitions(tree.topNode, slice));
      return target ? systemLink(safeHref(target.url) ?? "") : null;
    }
    if (node.name === "Autolink") {
      const url = node.getChild("URL");
      return url ? systemLink(safeHref(slice(url.from, url.to)) ?? "") : null;
    }
    if (node.name === "URL") bare = slice(node.from, node.to);
  }
  return bare ? systemLink(safeHref(bare) ?? "") : null;
}

export function editorLinkClick(view: EditorView, event: MouseEvent, bodyStart: number) {
  if (!(event.target instanceof Node) || !view.contentDOM.contains(event.target)) return null;
  const hit = view.posAndSideAtCoords({ x: event.clientX, y: event.clientY });
  if (!hit) return null;
  const char = hit.assoc === 1 ? hit.pos : hit.pos - 1;
  if (char < bodyStart || char >= view.state.doc.length) return null;
  // CodeMirror can return the end of a line when clicking the space beside it.
  const rect = view.coordsForChar(char);
  if (
    !rect ||
    event.clientX < rect.left ||
    event.clientX > rect.right ||
    event.clientY < rect.top ||
    event.clientY > rect.bottom
  )
    return null;
  return editorLinkAt(view.state, hit.pos, hit.assoc);
}
