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
import { type EditorState, StateEffect, StateField, type Text } from "@codemirror/state";
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

/** Matches counted before the count shows "999+". */
const COUNT_LIMIT = 999;
/** Typing pauses this long before a recount: a scan of a 2 MB note takes over 100 ms. */
const COUNT_DELAY = 150;

/** Up to `COUNT_LIMIT + 1` matches of the query, as [from, to] pairs. */
function matches(state: EditorState, query: SearchQuery): [number, number][] {
  const found: [number, number][] = [];
  const cursor = query.getCursor(state);
  for (let next = cursor.next(); !next.done && found.length <= COUNT_LIMIT; next = cursor.next())
    found.push([next.value.from, next.value.to]);
  return found;
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
    showCount(state);
  }
  // Matches are counted once per text and query, after a pause; moving between them reuses the count.
  let counted: { doc: Text; query: SearchQuery; found: [number, number][] } | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  function showCount(state: EditorState) {
    const query = getSearchQuery(state);
    if (!query.search || !query.valid) {
      clearTimeout(timer);
      count.textContent = "";
      return;
    }
    if (!counted || counted.doc !== state.doc || !counted.query.eq(query)) {
      count.textContent = "";
      clearTimeout(timer);
      timer = setTimeout(() => {
        counted = { doc: view.state.doc, query, found: matches(view.state, query) };
        showCount(view.state);
      }, COUNT_DELAY);
      return;
    }
    const { found } = counted;
    const { from, to } = state.selection.main;
    const at = found.findIndex(([start, end]) => start === from && end === to) + 1;
    count.textContent =
      found.length === 0
        ? t("search.none")
        : t("search.count", {
            at: at || "?",
            total: found.length > COUNT_LIMIT ? `${COUNT_LIMIT}+` : found.length,
          });
  }
  for (const input of [find, replace]) input.addEventListener("input", commit);
  dom.addEventListener("keydown", (event) => {
    // Ctrl+F again goes on to the panel's search bar.
    if (event.ctrlKey && event.key.toLowerCase() === "f") return;
    // Plain keys stay here: Tab moves through the panel, and list keys must not act on the cards.
    // Ctrl shortcuts such as Ctrl+E still reach the panel's handler.
    if (!event.ctrlKey) event.stopPropagation();
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
    // The first Ctrl+F or Ctrl+H must put typing in the field, not in the note.
    mount() {
      find.focus();
      find.select();
    },
    destroy() {
      clearTimeout(timer);
    },
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
