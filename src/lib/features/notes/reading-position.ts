// Maps a point in the reading view back to the note's text, so a double click edits where it was.
import { builtFrom, type Content, type Element, sourceRange } from "./render";

/** Where a double click in the reading view starts the editor. */
export type EditStart = {
  /** The note's text the reading view showed; positions apply only to it. */
  text: string;
  /** Position in the text, line breaks normalized as the editor counts them. */
  from: number;
  /** The word the reading view selected there, to select it in the editor too. */
  word: string;
  /** Viewport height the word was drawn at, where the editor keeps it. */
  top: number;
};

/**
 * Greedily finds `text`'s first `count` characters, in order, in `source` between `from` and `to`,
 * skipping what the reading view hides there: syntax marks, escapes, indentation. A character
 * that is not found, such as a decoded entity, is skipped. Returns where the match ends.
 */
function match(source: string, text: string, count: number, from: number, to: number) {
  const segment = source.slice(from, to);
  let at = 0;
  for (let i = 0; i < count; i++) {
    const found = segment.indexOf(text[i], at);
    if (found !== -1) at = found + 1;
  }
  return from + at;
}

/** The start of the first sourced element from `index` on, which text before it cannot pass. */
function nextStart(children: Content[], index: number) {
  for (let i = index; i < children.length; i++) {
    const child = children[i];
    const range = typeof child === "string" ? undefined : sourceRange(child);
    if (range) return range.from;
  }
  return undefined;
}

/** Where `item`, met at `cursor`, ends in the source. */
function skip(source: string, item: Content, cursor: number, bound: number): number {
  if (typeof item === "string") return match(source, item, item.length, cursor, bound);
  const range = sourceRange(item);
  if (range) return Math.max(cursor, range.to);
  const children = item.children ?? [];
  children.forEach((child, i) => {
    cursor = skip(source, child, cursor, nextStart(children, i + 1) ?? bound);
  });
  return cursor;
}

function locate(
  source: string,
  element: Element,
  path: number[],
  offset: number,
  cursor: number,
  limit: number,
): number {
  const range = sourceRange(element);
  if (range) {
    cursor = range.from;
    limit = range.to;
  }
  const [index, ...rest] = path;
  const children = element.children ?? [];
  if (index === undefined || index >= children.length) return cursor;
  for (let i = 0; i < index; i++)
    cursor = skip(source, children[i], cursor, nextStart(children, i + 1) ?? limit);
  const target = children[index];
  const bound = nextStart(children, index + 1) ?? limit;
  if (typeof target !== "string") return locate(source, target, rest, offset, cursor, bound);
  const before = match(source, target, offset, cursor, bound);
  // On a character, its own position: syntax right before it is not part of the word.
  if (offset >= target.length) return before;
  const at = source.slice(before, bound).indexOf(target[offset]);
  return at === -1 ? before : before + at;
}

/**
 * The position in `source` (line breaks normalized) of a point in rendered content: `offset`
 * characters into the text reached through `path`, child indices from `block`, a block of
 * `noteModel`. A point on an element rather than text is the element's start. Null when the block
 * does not know where it comes from.
 */
export function sourceOffset(
  source: string,
  block: Element,
  path: number[],
  offset: number,
): number | null {
  const range = sourceRange(block);
  return range ? locate(source, block, path, offset, range.from, range.to) : null;
}

/** The position in `source` of a DOM point inside `body`, the reading view's container. */
function domOffset(source: string, body: Node, node: Node, offset: number): number | null {
  if (node.nodeType !== Node.TEXT_NODE && offset < node.childNodes.length) {
    node = node.childNodes[offset];
    offset = 0;
  }
  const path: number[] = [];
  let current: Node = node;
  while (current.parentNode !== body) {
    const parent = current.parentNode;
    if (!parent) return null;
    path.unshift(Array.prototype.indexOf.call(parent.childNodes, current));
    current = parent;
  }
  const block = builtFrom(current);
  return block ? sourceOffset(source, block, path, offset) : null;
}

/** The nearest rendered block to a viewport height, for a point between blocks. */
function nearestBlock(body: HTMLElement, y: number): globalThis.Element | null {
  let best: globalThis.Element | null = null,
    distance = Number.POSITIVE_INFINITY;
  for (const child of body.children) {
    const rect = child.getBoundingClientRect();
    const away = y < rect.top ? rect.top - y : y > rect.bottom ? y - rect.bottom : 0;
    if (away < distance) {
      best = child;
      distance = away;
    }
  }
  return best;
}

/**
 * Where the editor should start after a double click in the reading view `body` showing `text`: the
 * word the browser selected, else the clicked point, else the nearest block's start.
 */
export function editStart(
  body: HTMLElement,
  text: string,
  event: MouseEvent,
): EditStart | undefined {
  const source = text.replace(/\r\n?/g, "\n");
  const selection = body.ownerDocument.getSelection();
  const range = selection?.rangeCount ? selection.getRangeAt(0) : null;
  if (range && body.contains(range.startContainer) && body !== range.startContainer) {
    const from = domOffset(source, body, range.startContainer, range.startOffset);
    const rect = range.getClientRects()[0];
    if (from !== null)
      return {
        text,
        from,
        // Windows selects the space after a double-clicked word too.
        word: selection?.toString().trim() ?? "",
        top: rect?.top ?? event.clientY,
      };
  }
  const block = nearestBlock(body, event.clientY);
  const from = block && domOffset(source, body, block, 0);
  if (block && from !== null)
    return { text, from, word: "", top: block.getBoundingClientRect().top };
  return undefined;
}
