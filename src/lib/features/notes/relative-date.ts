/** Calendar days take priority over elapsed hours, including midnight and DST boundaries. */
export function relativeDate(ms: number, now: number, locale: string): string {
  const date = new Date(ms);
  const today = new Date(now);
  const civilDay = (value: Date) =>
    Date.UTC(value.getFullYear(), value.getMonth(), value.getDate()) / 86_400_000;
  const days = civilDay(date) - civilDay(today);
  const age = Math.abs(days);
  let unit: Intl.RelativeTimeFormatUnit;
  let value: number;
  if (days !== 0) {
    unit = age < 7 ? "day" : age < 30 ? "week" : age < 365 ? "month" : "year";
    const size = unit === "day" ? 1 : unit === "week" ? 7 : unit === "month" ? 30 : 365;
    value = Math.trunc(days / size);
  } else {
    const elapsed = ms - now;
    const size =
      Math.abs(elapsed) < 60_000 ? 0 : Math.abs(elapsed) < 3_600_000 ? 60_000 : 3_600_000;
    unit = size === 0 ? "second" : size === 60_000 ? "minute" : "hour";
    value = size === 0 ? 0 : Math.trunc(elapsed / size);
  }
  return new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(value, unit);
}
