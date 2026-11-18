import { CompletionContext } from "@codemirror/autocomplete";
import { undo } from "@codemirror/commands";
import type { Transaction } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { afterEach, expect, it, vi } from "vitest";
import { setLocale } from "../../i18n";
import { editorExtensions, editorState } from "./editor";
import { noteParser } from "./markdown";
import { slashCompletion } from "./slash-commands";

afterEach(() => {
  if (typeof document !== "undefined") setLocale("en");
  vi.unstubAllGlobals();
});

function editor(marked: string, locale: "en" | "pt-BR" = "en") {
  vi.stubGlobal("document", { documentElement: { lang: locale } });
  setLocale(locale);
  const pos = marked.indexOf("|");
  let state = editorState(
    marked.replace("|", ""),
    editorExtensions(
      () => {},
      () => {},
      "Note",
    ),
  );
  state = state.update({ selection: { anchor: pos } }).state;
  const view = {
    get state() {
      return state;
    },
    dispatch(transaction: Transaction) {
      state = transaction.state;
    },
  };
  const list = slashCompletion(new CompletionContext(state, pos, false));
  return {
    view,
    list,
    choose(label: string) {
      const option = list?.options.find((option) => option.label === label);
      if (!list || !option || typeof option.apply !== "function")
        throw new Error(`Missing slash command ${label}`);
      option.apply(view as EditorView, option, list.from, list.to ?? pos);
    },
  };
}

it.each(["Text\n/|", "Text /|"])("offers commands at a slash boundary in %j", (text) => {
  expect(editor(text).list?.options.map(({ label }) => label)).toContain("Heading 2");
});

it.each([
  "and/|or",
  "https:/|/example.com",
  "[relative]( /|docs)",
  "`code /|`",
  "```\n/|\n```",
  "Text /unknown |",
])("leaves the slash literal in %j", (text) => {
  expect(editor(text).list).toBeNull();
});

it.each(["tí", "TI", "HEADING-2"])("filters Portuguese commands by %s", (query) => {
  expect(editor(`/${query}|`, "pt-BR").list?.options.map(({ label }) => label)).toContain(
    "Título 2",
  );
});

it("applies a heading and removes the query in a single undo step", () => {
  const { view, choose } = editor("/heading-2|Title");
  choose("Heading 2");
  expect(view.state.doc.toString()).toBe("## Title");
  expect(undo(view)).toBe(true);
  expect(view.state.doc.toString()).toBe("/heading-2Title");
  expect(undo(view)).toBe(false);
});

it.each(["Text\n/divider|", "Text /divider|"])("inserts a thematic break in %j", (text) => {
  const { view, choose } = editor(text);
  choose("Divider");
  expect(noteParser.parse(view.state.doc.toString()).toString()).toBe(
    "Document(Paragraph,HorizontalRule)",
  );
});

it("sorts only the non-blank block around the cursor and keeps frontmatter intact", () => {
  const { view, choose } = editor("---\ntitle: Keep\n---\n- Zebra\n- Ant /sort-ascending|\n\nKeep");
  choose("Sort A → Z");
  expect(view.state.doc.toString()).toBe("---\ntitle: Keep\n---\n- Ant \n- Zebra\n\nKeep");
});

it("offers only the commands that fit YAML inside the frontmatter", () => {
  const labels = editor("---\ntitle: /|\n---").list?.options.map(({ label }) => label);
  expect(labels).toContain("Today’s date");
  expect(labels).toContain("Add metadata");
  expect(labels).not.toContain("Heading 2");
  expect(labels).not.toContain("Sort A → Z");
});
