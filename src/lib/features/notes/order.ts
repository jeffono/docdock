import type { Note, NoteOrder } from "../../ipc";
import type { Hit } from "./search";
/** Search keeps relevance; only the unfiltered list uses the saved order. */
export function orderHits(hits: Hit[], notes: Note[], order: NoteOrder, query: string): Hit[] {
  if (query.trim()) return hits;
  const created = new Map(notes.map((note) => [note.path, note.created ?? note.modified]));
  const direction = order.endsWith("asc") ? 1 : -1;
  return [...hits].sort((a, b) => {
    const compared = order.startsWith("name")
      ? a.title.localeCompare(b.title)
      : order.startsWith("created")
        ? (created.get(a.path) ?? a.modified) - (created.get(b.path) ?? b.modified)
        : a.modified - b.modified;
    return direction * compared || (a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  });
}
