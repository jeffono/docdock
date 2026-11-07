import { history, undo } from "@codemirror/commands";
import { EditorState } from "@codemirror/state";
import { expect, test } from "vitest";
import { frontmatter, repairTags, tagProblems } from "./markdown";
import { diskReload, rawHistory, rawText } from "./reload";
import { tagDiagnostics } from "./tag-diagnostics";

const note = (value: string) => `---\ntags: ${value}\n---\nBody`;
const problems = (text: string) => {
  const lines = text.split(/\r\n|\n|\r/);
  return tagProblems((n) => lines[n - 1], lines.length);
};

test("repairs comma-separated plain words without dropping tags", () => {
  expect(repairTags(note("ação, next-step, two words"))).toBe(note("[ação, next-step, two words]"));
});

test("quotes colon-space flow items while retaining valid items and spacing", () => {
  const text = repairTags(note('[Plano: Q3, "a, b", next]'));
  expect(text).toBe(note('["Plano: Q3", "a, b", next]'));
  const lines = text.split("\n");
  expect(frontmatter((n) => lines[n - 1], lines.length)?.tags).toEqual([
    "Plano: Q3",
    "a, b",
    "next",
  ]);
});

test("repair preserves BOM, CRLF and untouched mixed line endings", () => {
  const text = "\uFEFF---\r\ntags:\t a, b  \r\ntitle: Keep\n---\rBody\r\n";
  expect(repairTags(text)).toBe(text.replace("a, b", "[a, b]"));
});

test.each([
  "a",
  '"a, b"',
  "[a, b]",
  "[]",
  "[a,]",
  "[a, 'it''s', \"a\\\", b\"]",
  "\n  - a\n  - 'b: c'",
  "a # comment",
])("valid tags stay byte-for-byte unchanged: %s", (value) => {
  expect(repairTags(note(value))).toBe(note(value));
  expect(problems(note(value))).toEqual([]);
});

test.each([
  "Plano: Q3",
  "a,, b",
  "[Plano: Q3, {a: b}]",
  "[a,",
  '["unfinished]',
  "a, b # comment",
  "|\n  a, b",
  "a, b\n  continuation",
])("ambiguous tags are marked without rewriting: %s", (value) => {
  expect(repairTags(note(value))).toBe(note(value));
  expect(problems(note(value))[0]).toEqual({ line: 2, replacement: null });
});

test("unreadable block items are marked without treating nested data as a tag", () => {
  const text = note("\n  - Plano: Q3");
  expect(problems(text)).toEqual([{ line: 3, replacement: null }]);
  expect(repairTags(text)).toBe(text);
});

test("duplicate tag keys stay untouched and are both marked", () => {
  const text = note("a, b\ntag: c, d");
  expect(problems(text)).toEqual([
    { line: 2, replacement: null },
    { line: 3, replacement: null },
  ]);
  expect(repairTags(text)).toBe(text);
});

test("tag-like body text and unclosed frontmatter are never repaired or marked", () => {
  for (const text of ["tags: a, b", "---\ntags: a, b", "---\ntitle: A\n---\ntags: a, b"]) {
    expect(repairTags(text)).toBe(text);
    expect(problems(text)).toEqual([]);
  }
});

test("repair is one undoable change with the original bytes recoverable", () => {
  const original = "\uFEFF---\r\ntags: a, b\r\n---\nBody";
  let state = EditorState.create({
    doc: original,
    extensions: [history(), rawText.init(() => original), rawHistory, tagDiagnostics],
  });
  // Diagnostics only mark; they must never rewrite while the user types.
  expect(state.field(rawText)).toBe(original);
  expect(state.field(tagDiagnostics).size).toBe(1);
  state = state.update(diskReload(state, repairTags(original))).state;
  expect(state.field(tagDiagnostics).size).toBe(0);
  expect(
    undo({
      state,
      dispatch: (tr) => {
        state = tr.state;
      },
    }),
  ).toBe(true);
  expect(state.field(rawText)).toBe(original);
});

// Bug: an unpaired quote in a flow list dropped every tag of the note.
test("a flow list with an apostrophe still reads its other tags", () => {
  const lines = note("[don't, music]").split("\n");
  expect(frontmatter((n) => lines[n - 1], lines.length)?.tags).toContain("music");
});
