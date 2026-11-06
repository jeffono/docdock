/** Minimal bridge from search to a web address; ordinary note queries stay searches. */
export function parseAddress(text: string): string | null {
  const raw = text.trim();
  if (!raw || /\s/.test(raw)) return null;
  const explicit = /^https?:\/\//i.test(raw);
  if (!explicit && !/^[^/:?#@]+\.[^/:?#@]+(?::\d+)?(?:[/?#]|$)/u.test(raw)) return null;
  try {
    const url = new URL(explicit ? raw : `https://${raw}`);
    if (
      !["http:", "https:"].includes(url.protocol) ||
      !url.hostname ||
      url.username ||
      url.password
    )
      return null;
    return url.href;
  } catch {
    return null;
  }
}
