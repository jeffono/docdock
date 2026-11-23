import { frontmatter } from "../notes/markdown";

const MAX_HEADER_LINES = 64;
const MAX_HEADER_CHARS = 4_000;
const MAX_PREVIEW_TAGS = 24;
const MAX_TITLE_CHARS = 200;
const MAX_TAG_CHARS = 64;

/**
 * Truncates large notes and oversized frontmatter before side-preview parsing and DOM rendering
 * so hovering a multi-megabyte file stays instantaneous. Closes any open fenced code block.
 */
export function clipPreviewNote(text: string, maxLines = 100, maxChars = 12_000): string {
  const normalized = text.replace(/\r\n?/g, "\n");
  const lines = normalized.split("\n");
  const meta = frontmatter((n) => lines[n - 1], lines.length);
  let headerLines = meta ? lines.slice(0, meta.end) : [];
  let headerClipped = false;
  if (meta) {
    const headerChars = headerLines.reduce((sum, line) => sum + line.length + 1, 0);
    const oversizedHeader =
      headerLines.length > MAX_HEADER_LINES ||
      headerChars > MAX_HEADER_CHARS ||
      (meta.title?.length ?? 0) > MAX_TITLE_CHARS ||
      meta.tags.length > MAX_PREVIEW_TAGS ||
      meta.tags.some((tag) => tag.length > MAX_TAG_CHARS);
    if (oversizedHeader) {
      headerClipped = true;
      const parts = ["---"];
      if (meta.title) {
        parts.push(`title: ${JSON.stringify(meta.title.slice(0, MAX_TITLE_CHARS))}`);
      }
      if (meta.tags.length) {
        const tags = meta.tags
          .slice(0, MAX_PREVIEW_TAGS)
          .map((tag) => JSON.stringify(tag.slice(0, MAX_TAG_CHARS)));
        parts.push(`tags: [${tags.join(", ")}]`);
      }
      parts.push("---");
      headerLines = parts;
    }
  }
  const bodyLines = meta ? lines.slice(meta.end) : lines;

  const kept: string[] = [];
  let chars = 0;
  let truncated = false;
  for (const line of bodyLines) {
    if (kept.length >= maxLines || (kept.length > 0 && chars + line.length > maxChars)) {
      truncated = true;
      break;
    }
    if (line.length > maxChars) {
      kept.push(line.slice(0, maxChars));
      truncated = true;
      break;
    }
    kept.push(line);
    chars += line.length + 1;
  }

  if (!truncated && !headerClipped) return text;

  let fence: { char: "`" | "~"; len: number } | null = null;
  for (const line of kept) {
    const match = line.match(/^[ \t]{0,3}(`{3,}|~{3,})(.*)$/);
    if (!match) continue;
    const marker = match[1];
    const rest = match[2];
    const ch = marker[0] as "`" | "~";
    if (!fence) {
      if (ch === "`" && rest.includes("`")) continue;
      fence = { char: ch, len: marker.length };
    } else if (ch === fence.char && marker.length >= fence.len && /^[ \t]*$/.test(rest)) {
      fence = null;
    }
  }
  if (fence) kept.push(fence.char.repeat(fence.len));
  return [...headerLines, ...kept].join("\n");
}
