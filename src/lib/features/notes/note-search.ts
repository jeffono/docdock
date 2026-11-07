// Find and replace in the note: one compact row at the top of the editor, the replace row on demand.
import {
  closeSearchPanel,
  findNext,
  findPrevious,
  getSearchQuery,
  openSearchPanel,
  replaceAll,
  replaceNext,
  SearchQuery,
  search,
  searchKeymap,
  setSearchQuery,
} from "@codemirror/search";
import { type EditorState, StateEffect, StateField } from "@codemirror/state";
import {
  type Command,
  type EditorView,
  type KeyBinding,
  type Panel,
  runScopeHandlers,
} from "@codemirror/view";
import { t } from "../../i18n";

const showReplace = StateEffect.define<boolean>();
const replacing = StateField.define<boolean>({
  create: () => false,
  update(value, transaction) {
    for (const effect of transaction.effects) if (effect.is(showReplace)) value = effect.value;
    return value;
  },
});

const open =
  (replace: boolean): Command =>
  (view) => {
    view.dispatch({ effects: showReplace.of(replace) });
    return openSearchPanel(view);
  };

/** Matches counted before the count shows "999+"; a long note must not slow typing. */
const COUNT_LIMIT = 999;

/** The match holding the main selection, 1-based or 0, and how many there are; null for no query. */
export function matchCount(state: EditorState): { at: number; total: number } | null {
  const query = getSearchQuery(state);
  if (!query.valid) return null;
  const { from, to } = state.selection.main;
  const cursor = query.getCursor(state);
  let total = 0;
  let at = 0;
  for (let next = cursor.next(); !next.done && total <= COUNT_LIMIT; next = cursor.next()) {
    total++;
    if (next.value.from === from && next.value.to === to) at = total;
  }
  return { at, total };
}

// Lucide paths (ISC license), as in `ui/Icon.svelte`, which cannot render inside the editor's shadow root.
const icons = {
  chevronRight: "m9 18 6-6-6-6",
  chevronDown: "m6 9 6 6 6-6",
  chevronUp: "m18 15-6-6-6 6",
  x: "M18 6 6 18 M6 6l12 12",
};

function button(label: string, content: keyof typeof icons | string, run: () => void) {
  const element = document.createElement("button");
  element.type = "button";
  element.title = label;
  element.setAttribute("aria-label", label);
  if (content in icons) {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("aria-hidden", "true");
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", icons[content as keyof typeof icons]);
    svg.append(path);
    element.append(svg);
  } else element.textContent = content;
  element.addEventListener("click", run);
  return element;
}

function field(label: string) {
  const input = document.createElement("input");
  input.type = "text";
  input.placeholder = label;
  input.spellcheck = false;
  input.setAttribute("aria-label", label);
  return input;
}

function searchPanel(view: EditorView): Panel {
  const find = field(t("search.find"));
  find.setAttribute("main-field", "true");
  const replace = field(t("search.replace"));
  const count = document.createElement("span");
  count.className = "count";
  count.setAttribute("aria-live", "polite");
  const toggle = button(t("search.toggleReplace"), "chevronRight", () =>
    view.dispatch({ effects: showReplace.of(!view.state.field(replacing)) }),
  );
  const matchCase = button(t("search.matchCase"), "Aa", () => {
    matchCase.ariaPressed = String(matchCase.ariaPressed !== "true");
    commit();
  });
  const regexp = button(t("search.regexp"), ".*", () => {
    regexp.ariaPressed = String(regexp.ariaPressed !== "true");
    commit();
  });
  matchCase.className = regexp.className = "option";
  const row = document.createElement("div");
  row.className = "row";
  row.append(
    toggle,
    find,
    count,
    matchCase,
    regexp,
    button(t("search.previous"), "chevronUp", () => findPrevious(view)),
    button(t("search.next"), "chevronDown", () => findNext(view)),
    button(t("search.close"), "x", () => closeSearchPanel(view)),
  );
  const replaceRow = document.createElement("div");
  replaceRow.className = "row replace";
  replaceRow.append(
    replace,
    button(t("search.replaceOne"), t("search.replaceOne"), () => replaceNext(view)),
    button(t("search.replaceAll"), t("search.replaceAll"), () => replaceAll(view)),
  );
  const dom = document.createElement("div");
  dom.className = "note-search";
  dom.append(row, replaceRow);

  function commit() {
    const query = new SearchQuery({
      search: find.value,
      replace: replace.value,
      caseSensitive: matchCase.ariaPressed === "true",
      regexp: regexp.ariaPressed === "true",
      literal: true,
    });
    if (!query.eq(getSearchQuery(view.state))) view.dispatch({ effects: setSearchQuery.of(query) });
  }
  function show(state: EditorState) {
    const query = getSearchQuery(state);
    if (find.value !== query.search) find.value = query.search;
    if (replace.value !== query.replace) replace.value = query.replace;
    matchCase.ariaPressed = String(query.caseSensitive);
    regexp.ariaPressed = String(query.regexp);
    const shown = state.field(replacing);
    replaceRow.hidden = !shown;
    toggle.ariaExpanded = String(shown);
    toggle.querySelector("path")?.setAttribute("d", shown ? icons.chevronDown : icons.chevronRight);
    const counted = query.search ? matchCount(state) : null;
    count.textContent = !counted
      ? ""
      : counted.total === 0
        ? t("search.none")
        : t("search.count", {
            at: counted.at || "?",
            total: counted.total > COUNT_LIMIT ? `${COUNT_LIMIT}+` : counted.total,
          });
  }
  for (const input of [find, replace]) input.addEventListener("input", commit);
  dom.addEventListener("keydown", (event) => {
    // Ctrl+F again goes on to the panel's search bar.
    if (event.ctrlKey && event.key.toLowerCase() === "f") return;
    if (runScopeHandlers(view, event, "search-panel")) {
      event.preventDefault();
      event.stopPropagation();
    } else if (event.key === "Enter" && event.target === find) {
      event.preventDefault();
      (event.shiftKey ? findPrevious : findNext)(view);
    } else if (event.key === "Enter" && event.target === replace) {
      event.preventDefault();
      replaceNext(view);
    }
  });
  show(view.state);
  return {
    dom,
    top: true,
    update(update) {
      const changed =
        update.docChanged ||
        update.selectionSet ||
        update.startState.field(replacing) !== update.state.field(replacing) ||
        !getSearchQuery(update.startState).eq(getSearchQuery(update.state));
      if (changed) show(update.state);
    },
  };
}

/** Ctrl+F finds and Ctrl+H replaces in the note; Esc, F3 and Ctrl+D come from CodeMirror. */
export const noteSearchKeymap: KeyBinding[] = [
  { key: "Mod-f", run: open(false), scope: "editor search-panel" },
  { key: "Mod-h", run: open(true), scope: "editor search-panel" },
  // Go to line opens CodeMirror's own unstyled dialog.
  ...searchKeymap.filter((binding) => binding.key !== "Mod-Alt-g" && binding.key !== "Mod-f"),
];

export const noteSearch = () => [replacing, search({ createPanel: searchPanel, literal: true })];
