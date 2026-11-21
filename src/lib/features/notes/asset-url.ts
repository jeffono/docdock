// Apart from the Markdown rules so the image preview window loads it without the parser.

/** Asset URL for a path already relative to the notes folder, with literal file names. */
export function noteAssetUrl(path: string): string {
  return `http://note-asset.localhost/${path
    .split("/")
    .map((p) =>
      encodeURIComponent(p).replace(
        /[!'()*]/g,
        (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`,
      ),
    )
    .join("/")}`;
}
