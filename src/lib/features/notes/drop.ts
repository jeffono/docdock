/** What a drag from another app brings that a note can take: image files, else plain text. */
export type Dropped = { images: File[] } | { text: string };

const imageFile = (file: File) =>
  file.type.startsWith("image/") || (!file.type && /\.(gif|jpe?g|png|webp)$/i.test(file.name));

/** Whether a drag may bring something a note can take; while dragging only its types can be read. */
export function bringsContent(types: readonly string[]): boolean {
  return types.includes("Files") || types.includes("text/plain");
}

/**
 * What was dropped, read during the drop event, since its data is gone after it. Files win over text,
 * which a browser adds to an image as its address. Null when there is nothing; `unsupported` when a
 * file is not an image, since only images are attached.
 */
export function droppedContent(data: DataTransfer | null): Dropped | "unsupported" | null {
  if (!data) return null;
  const files = Array.from(data.files);
  if (files.length) return files.every(imageFile) ? { images: files } : "unsupported";
  const text = data.getData("text/plain").replace(/\r\n?/g, "\n");
  return text.trim() ? { text } : null;
}

/** The Markdown for images attached at `links`: one per line, so they show as one grid. */
export function imagesBlock(links: readonly string[]): string {
  return links.map((link) => `![](${link})`).join("\n");
}

/**
 * `note` with `block` added at its end as a paragraph of its own, in the note's line endings; the
 * note's own text is never changed.
 */
export function appended(note: string, block: string): string {
  const newline = note.match(/\r\n|\n|\r/)?.[0] ?? "\n";
  const body = block.replace(/\n+$/, "").replace(/\n/g, newline);
  if (!note) return body + newline;
  // Counted from the end by hand: a regex over the line breaks backtracks badly on long runs.
  let end = note.length;
  let breaks = 0;
  while (breaks < 2 && end > 0) {
    if (note.startsWith("\r\n", end - 2)) end -= 2;
    else if (note[end - 1] === "\n" || note[end - 1] === "\r") end -= 1;
    else break;
    breaks++;
  }
  return note + newline.repeat(2 - breaks) + body + newline;
}

/** The largest image Rust attaches, as its `MAX_ATTACHMENT_BYTES`: larger ones are not read. */
export const IMAGE_LIMIT_MB = 10;

/** Fails as Rust would for an image over the limit, before its bytes are read into memory. */
export function checkImageSize(image: File) {
  if (image.size > IMAGE_LIMIT_MB * 1024 * 1024)
    throw { code: "imageTooLarge", limitMb: IMAGE_LIMIT_MB };
}
