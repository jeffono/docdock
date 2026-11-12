import { expect, test } from "vitest";
import { dateRange, metadataDate } from "./query-date";

const now = new Date(2026, 0, 2, 15);
const range = (from: Date, until: Date) => ({ from: +from, until: +until });
test("last seven days includes today across a year boundary", () => {
  expect(dateRange("7d", now)).toEqual(range(new Date(2025, 11, 27), new Date(2026, 0, 3)));
});
test("today uses local midnight rather than UTC", () => {
  expect(dateRange("today", now)).toEqual(range(new Date(2026, 0, 2), new Date(2026, 0, 3)));
});
test.each([
  ["month", new Date(2026, 0, 1), new Date(2026, 1, 1)],
  ["last-month", new Date(2025, 11, 1), new Date(2026, 0, 1)],
])("%s uses calendar months", (token, from, until) => {
  expect(dateRange(token as string, now)).toEqual(range(from as Date, until as Date));
});
test.each([
  ["2026", new Date(2026, 0, 1), new Date(2027, 0, 1)],
  ["2026-10", new Date(2026, 9, 1), new Date(2026, 10, 1)],
  ["2024-02-29", new Date(2024, 1, 29), new Date(2024, 2, 1)],
  ["2026-10..2026-11", new Date(2026, 9, 1), new Date(2026, 11, 1)],
])("%s includes its entire final period", (token, from, until) => {
  expect(dateRange(token as string, now)).toEqual(range(from as Date, until as Date));
});
test("open ranges have no artificial endpoint", () => {
  expect(dateRange("2026-10..", now)).toEqual({ from: +new Date(2026, 9, 1), until: Infinity });
  expect(dateRange("..2026", now)).toEqual({ from: -Infinity, until: +new Date(2027, 0, 1) });
});
test.each(["2026-02-29", "2026-13", "2026-04-31", "2026-11..2026-10", "..", "bad", "2026..bad"])(
  "ignores invalid date %s",
  (token) => {
    expect(dateRange(token, now)).toBeNull();
  },
);
test("day-only metadata stays on its local calendar day", () => {
  expect(metadataDate("2026-10-03")).toBe(+new Date(2026, 9, 3));
});
test("timestamp metadata respects its explicit offset", () => {
  expect(metadataDate("2026-10-03T23:30:00-03:00")).toBe(Date.parse("2026-10-04T02:30:00Z"));
});
test.each(["2026-02-30T12:00:00Z", "2026-10-03T27:00:00Z", "yesterday"])(
  "invalid metadata %s falls back",
  (value) => {
    expect(metadataDate(value)).toBeNull();
  },
);
