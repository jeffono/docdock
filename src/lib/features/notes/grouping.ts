import type { Note, NoteOrder } from "../../ipc";
import type { Hit } from "./search";

export type DateGroup = {
  key: string;
  kind: "today" | "yesterday" | "week" | "month" | "calendarMonth" | "year";
  date: number;
};

// Compare civil dates rather than elapsed hours, so DST days still count as one day.
const civilDay = (date: Date) =>
  Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86_400_000;

/** Headers for already ordered, unpinned hits. No effects or changes to selection keys. */
export function dateHeadings(
  hits: Hit[],
  notes: Note[],
  order: NoteOrder,
  enabled: boolean,
  query: string,
  now: Date,
): Map<string, DateGroup> {
  const headings = new Map<string, DateGroup>();
  if (!enabled || query.trim() || order.startsWith("name")) return headings;
  const created = new Map(notes.map((note) => [note.path, note.created ?? note.modified]));
  let previous = "";
  for (const hit of hits) {
    const timestamp = order.startsWith("created")
      ? (created.get(hit.path) ?? hit.modified)
      : hit.modified;
    const date = new Date(timestamp);
    const age = civilDay(now) - civilDay(date);
    const kind: DateGroup["kind"] =
      age <= 0
        ? "today"
        : age === 1
          ? "yesterday"
          : age <= 7
            ? "week"
            : age <= 30
              ? "month"
              : date.getFullYear() === now.getFullYear()
                ? "calendarMonth"
                : "year";
    const key =
      kind === "calendarMonth"
        ? `month:${date.getFullYear()}:${date.getMonth()}`
        : kind === "year"
          ? `year:${date.getFullYear()}`
          : kind;
    if (key !== previous) headings.set(hit.path, { key, kind, date: timestamp });
    previous = key;
  }
  return headings;
}

export type FolderGroup = { key: string; folder: string };

/** The folder of a note path, "" at the root. */
export const folderOf = (path: string) => path.slice(0, Math.max(path.lastIndexOf("/"), 0));

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" }).compare;
/** By segment, so "a/b" follows "a" even when "a-x" sorts between them as text. */
export function folderCompare(a: string, b: string): number {
  const left = a.split("/");
  const right = b.split("/");
  for (let i = 0; i < Math.min(left.length, right.length); i++) {
    const order =
      collator(left[i], right[i]) || (left[i] < right[i] ? -1 : left[i] > right[i] ? 1 : 0);
    if (order) return order;
  }
  return left.length - right.length;
}
