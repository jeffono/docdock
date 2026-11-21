import { noteParser } from "./markdown";

function imageAtEdge(line: string, side: -1 | 1): boolean {
  const pos = side === -1 ? line.length : 0;
  const mark = noteParser.parse(line).resolve(pos, side);
  const image = mark.parent;
  if (mark.name !== "LinkMark" || image?.name !== "Image") return false;
  // The third mark opens an inline destination. Reference images lack that mark.
  const destination = image.getChildren("LinkMark")[2];
  return destination !== undefined && line[destination.from] === "(";
}

/** Format attachments using the line fragments immediately before and after the selection. */
export function imageInsertion(before: string, after: string, attachments: readonly string[]) {
  if (!attachments.length) return { insert: "", cursorOffset: 0 };
  const left = before.trimEnd();
  const right = after.trimStart();
  const prefix = left.endsWith(")") && imageAtEdge(left, -1) ? "\n" : "";
  const suffix = right.startsWith("![") && imageAtEdge(right, 1) ? "\n" : "";
  const images = attachments.map((path) => `![](${path})`).join("\n");
  const inserted = prefix + images;
  return { insert: inserted + suffix, cursorOffset: inserted.length };
}
