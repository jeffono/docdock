import { expect, test } from "vitest";
import { relativeDate } from "./relative-date";

// Local constructors keep midnight boundaries meaningful in every test runner timezone.
const now = new Date(2026, 10, 12, 12).getTime();
test.each([
  [now - 59_999, "now"],
  [now - 60_000, "1 minute ago"],
  [now - 3_599_999, "59 minutes ago"],
  [now - 3_600_000, "1 hour ago"],
  [now - 7 * 86_400_000, "last week"],
  [now - 30 * 86_400_000, "last month"],
  [now - 365 * 86_400_000, "last year"],
])("formats the unit boundary at %i", (ms, expected) => {
  expect(relativeDate(ms, now, "en")).toBe(expected);
});

test("uses yesterday across midnight even when only minutes elapsed", () => {
  expect(
    relativeDate(
      new Date(2026, 10, 11, 23, 55).getTime(),
      new Date(2026, 10, 12, 0, 5).getTime(),
      "en",
    ),
  ).toBe("yesterday");
});

test("keeps hours for dates on the same calendar day", () => {
  expect(relativeDate(new Date(2026, 10, 12, 10).getTime(), now, "en")).toBe("2 hours ago");
});

test("formats in the app locale", () => {
  expect(relativeDate(now - 2 * 3_600_000, now, "pt-BR")).toBe("há 2 horas");
});

test("handles a future calendar date", () => {
  expect(relativeDate(new Date(2026, 10, 13, 0, 5).getTime(), now, "en")).toBe("tomorrow");
});

test("shows a note saved after the last refresh tick as now, even past midnight", () => {
  const tick = new Date(2026, 10, 11, 23, 59, 50).getTime();
  expect(relativeDate(new Date(2026, 10, 12, 0, 0, 10).getTime(), tick, "en")).toBe("now");
});
