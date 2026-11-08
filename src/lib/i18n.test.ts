import { afterEach, expect, test, vi } from "vitest";
import { folderCountText, setLocale } from "./i18n";

afterEach(() => {
  setLocale("en");
  vi.unstubAllGlobals();
});
test.each([
  ["en", 1, 1, "1 note and 1 other file"],
  ["en", 3, 2, "3 notes and 2 other files"],
  ["pt-BR", 1, 1, "1 nota e 1 outro arquivo"],
  ["pt-BR", 0, 2, "0 notas e 2 outros arquivos"],
] as const)(
  "folder confirmation in %s counts %i notes and %i other files",
  (locale, notes, other, expected) => {
    vi.stubGlobal("document", { documentElement: { lang: "" } });
    setLocale(locale);
    expect(folderCountText({ notes, other })).toBe(expected);
  },
);
