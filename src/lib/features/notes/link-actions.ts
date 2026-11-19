type LinkGesture = Pick<MouseEvent, "type" | "button" | "shiftKey" | "ctrlKey" | "altKey">;

/** Accept only explicit web and email URLs, without resolving against a base. */
export function systemLink(href: string): URL | null {
  // Reject control characters and backslashes before the URL parser can normalize them.
  if (!/^(https?:\/\/|mailto:)/i.test(href) || /[\\\p{Cc}]/u.test(href)) return null;
  try {
    return new URL(href);
  } catch {
    return null;
  }
}

export function linkAction(event: LinkGesture, reading: boolean) {
  const press = event.type === "mousedown";
  if (event.button === 1 && (press || event.type === "auxclick")) return "external";
  if (event.button !== 0 || (!press && event.type !== "click")) return null;
  if (reading) return event.shiftKey || event.ctrlKey ? "external" : "here";
  // Alt+click (including with Shift held) is reserved for adding editor cursors.
  return event.shiftKey && !event.altKey ? "external" : null;
}

export function linkMenuKey(event: KeyboardEvent): boolean {
  return (
    event.key === "ContextMenu" ||
    (event.key === "F10" && event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey)
  );
}
