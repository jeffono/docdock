// Calendar boundaries are local; ranges use an exclusive upper bound to include the whole last day.
export type DateRange = { from: number; until: number };
export const datePresets = ["today", "7d", "month", "last-month"] as const;

function local(year: number, month = 1, day = 1): Date {
  const date = new Date(0);
  date.setFullYear(year, month - 1, day);
  date.setHours(0, 0, 0, 0);
  return date;
}

function isoPeriod(value: string): DateRange | null {
  const match = /^(\d{4})(?:-(\d{2})(?:-(\d{2}))?)?$/.exec(value);
  if (!match) return null;
  const year = Number(match[1]),
    month = Number(match[2] ?? 1),
    day = Number(match[3] ?? 1);
  const start = local(year, month, day);
  if (start.getFullYear() !== year || start.getMonth() !== month - 1 || start.getDate() !== day)
    return null;
  const end = new Date(start);
  if (match[3]) end.setDate(end.getDate() + 1);
  else if (match[2]) end.setMonth(end.getMonth() + 1);
  else end.setFullYear(end.getFullYear() + 1);
  return { from: +start, until: +end };
}

/** Invalid and reversed ranges are ignored by the caller. `now` is supplied, never read here. */
export function dateRange(value: string, now: Date): DateRange | null {
  const today = local(now.getFullYear(), now.getMonth() + 1, now.getDate());
  if (value === "today" || value === "7d") {
    const end = new Date(today);
    end.setDate(end.getDate() + 1);
    if (value === "7d") today.setDate(today.getDate() - 6);
    return { from: +today, until: +end };
  }
  if (value === "month" || value === "last-month") {
    const start = local(now.getFullYear(), now.getMonth() + (value === "month" ? 1 : 0));
    const end = new Date(start);
    end.setMonth(end.getMonth() + 1);
    return { from: +start, until: +end };
  }
  if (!value.includes("..")) return isoPeriod(value);
  const parts = value.split("..");
  if (parts.length !== 2 || !parts.some(Boolean)) return null;
  const start = parts[0] ? isoPeriod(parts[0]) : null;
  const end = parts[1] ? isoPeriod(parts[1]) : null;
  if ((parts[0] && !start) || (parts[1] && !end)) return null;
  const from = start?.from ?? -Infinity,
    until = end?.until ?? Infinity;
  return from < until ? { from, until } : null;
}

/** Frontmatter accepts a full calendar day or ISO timestamp, with optional time zone. */
export function metadataDate(value: string | null | undefined): number | null {
  if (!value) return null;
  const day = /^(\d{4}-\d{2}-\d{2})(?:$|[Tt ])/.exec(value);
  if (!day || !isoPeriod(day[1])) return null;
  if (value === day[1]) return isoPeriod(value)?.from ?? null;
  if (
    !/^\d{4}-\d{2}-\d{2}[Tt ]\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:[Zz]|[+-]\d{2}:?\d{2})?$/.test(
      value,
    )
  )
    return null;
  const time = Date.parse(value);
  return Number.isFinite(time) ? time : null;
}
