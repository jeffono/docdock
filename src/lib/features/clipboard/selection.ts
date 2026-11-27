import type { ClipboardEntry } from "../../ipc";

export function newestNewId(
  previous: readonly Pick<ClipboardEntry, "id">[],
  next: readonly Pick<ClipboardEntry, "id">[],
): string | null {
  const newest = next[0];
  return newest && !previous.some((item) => item.id === newest.id) ? newest.id : null;
}
