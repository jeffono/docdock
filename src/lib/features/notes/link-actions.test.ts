import { describe, expect, it } from "vitest";
import { linkAction, systemLink } from "./link-actions";

describe("system link destination", () => {
  it.each(["http://example.com/", "https://example.com/", "mailto:note@example.com"])(
    "accepts %s",
    (href) => expect(systemLink(href)?.href).toBe(href),
  );

  it.each([
    "../notes/other.md",
    "other.md",
    "#section",
    "/notes/other.md",
    "C:/notes/other.md",
    "\\\\server\\share\\other.md",
    "//example.com/other.md",
    "file:///C:/notes/other.md",
    "javascript:alert(1)",
    "data:text/plain,hello",
    "https://",
    "ht\ttps://example.com",
    "https://ex\nample.com",
    "https://example.com\\other.md",
  ])("rejects %j", (href) => expect(systemLink(href)).toBeNull());

  it("canonicalizes a web URL before passing it to the system", () => {
    expect(systemLink("HTTPS://example.com/two words")?.href).toBe(
      "https://example.com/two%20words",
    );
  });
});

const left = { type: "click", button: 0, shiftKey: false, ctrlKey: false, altKey: false };

describe.each([
  { view: "reading", reading: true },
  { view: "editing", reading: false },
])("link gestures while $view", ({ reading }) => {
  it("opens externally on Shift+click", () => {
    expect(linkAction({ ...left, shiftKey: true }, reading)).toBe("external");
  });

  it("opens externally on middle auxclick", () => {
    expect(linkAction({ ...left, type: "auxclick", button: 1 }, reading)).toBe("external");
  });

  it("does not activate on right auxclick", () => {
    expect(linkAction({ ...left, type: "auxclick", button: 2 }, reading)).toBeNull();
  });
});

it("opens a plain reading click in the panel", () => {
  expect(linkAction(left, true)).toBe("here");
});

it("preserves Ctrl+click in reading", () => {
  expect(linkAction({ ...left, ctrlKey: true }, true)).toBe("external");
});

it("leaves a plain editor click to CodeMirror", () => {
  expect(linkAction(left, false)).toBeNull();
});

it("leaves Shift+Alt+click available for adding cursors", () => {
  expect(linkAction({ ...left, shiftKey: true, altKey: true }, false)).toBeNull();
});
