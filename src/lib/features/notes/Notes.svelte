<script lang="ts">
import { onMount, tick, untrack } from "svelte";
import { cubicOut } from "svelte/easing";
import { dateTime, errorText, t } from "../../i18n";
import {
  type Leftover,
  type Note,
  type NotesChanges,
  type NotesFolder,
  notesChoose,
  notesFolder,
  notesLeftovers,
  notesList,
  notesOpen,
  notesRename,
  notesRestore,
  notesSave,
  notesTexts,
  type OpenNote,
  onNotesChanged,
  onPanelHidden,
  onPanelSearch,
  openExternal,
  type Pinned,
  panelClose,
  pinnedAdd,
  pinnedList,
  pinnedMove,
  pinnedRemove,
  pinnedRestore,
  pinnedSetCollapsed,
  webAddresses,
  webOpen,
  webOverlay,
  webSelection,
  webSetVisible,
  webSuggestions,
} from "../../ipc";
import Button from "../../ui/Button.svelte";
import Icon from "../../ui/Icon.svelte";
import IconButton from "../../ui/IconButton.svelte";
import TextField from "../../ui/TextField.svelte";
import {
  barDefault,
  downOpensHistory,
  historyEntries,
  LatestQuery,
  searchAddress,
  type WebEntry,
  webEntries,
  webQuery,
} from "../web/suggestions";
import type { AutosaveState } from "./autosave";
import CardView from "./Card.svelte";
import { dragSlot } from "./drag";
import Editor from "./Editor.svelte";
import { dateHeadings } from "./grouping";
import {
  arrowScroll,
  buildList,
  type Card,
  deeper,
  defaultKey,
  enter,
  entryKey,
  isMissing,
  isUntitled,
  type Level,
  move,
  navKeys,
  noteFileName,
  renamedPath,
  revealScroll,
  rows,
  shallower,
} from "./list";
import { retitle } from "./markdown";
import NoteOrder from "./NoteOrder.svelte";
import { orderHits } from "./order";
import { groupByDate, noteOrder } from "./order-store";
import { pageNote } from "./page";
import { noteName } from "./render";
import { FolderIndex, isPinnedList, search } from "./search";

let {
  browser = false,
  pageUrl = "",
  inactive = false,
}: { browser?: boolean; pageUrl?: string; inactive?: boolean } = $props();

let folder = $state<NotesFolder | null>(null);
let notes = $state<Note[]>([]);
let error = $state("");
let query = $state("");
let address = $state("");
let addressEditing = $state(false);
let showingHistory = $state(false);
let history = $state<string[]>([]);
let remote = $state<string[]>([]);
let suggestionsElement = $state<HTMLElement>();
const latest = new LatestQuery();
$effect(() => {
  void browser;
  showingHistory = false;
});
const inputQuery = $derived(browser ? address : query);
const intent = $derived(webQuery(inputQuery, browser));
const webItems = $derived(webEntries(inputQuery, browser, history, remote));
$effect(() => {
  if (browser && !addressEditing) address = pageUrl;
});
$effect(() => {
  const text = intent;
  const revision = latest.next();
  remote = [];
  if (showingHistory || text === null || text.length < 2 || (browser && !addressEditing)) return;
  const timer = setTimeout(() => {
    void webSuggestions(text)
      .then((items) => {
        if (latest.accepts(revision)) remote = items;
      })
      .catch(() => {});
  }, 180);
  return () => {
    clearTimeout(timer);
    latest.next();
  };
});
$effect(() => {
  void pageUrl;
  void browser;
  void webAddresses()
    .then((items) => {
      history = items;
    })
    .catch(() => {});
});
$effect(() => {
  const element = suggestionsElement;
  if (!element || !browser || !addressEditing) {
    void webOverlay(0);
    return;
  }
  const observer = new ResizeObserver(() => {
    void webOverlay(Math.ceil(element.getBoundingClientRect().height));
  });
  observer.observe(element);
  return () => {
    observer.disconnect();
    void webOverlay(0);
  };
});
async function activateWeb(entry: WebEntry) {
  showingHistory = false;
  if (entry.kind === "intent") {
    query = `? ${entry.text}`;
    selection = "";
    bar()?.focus();
    return;
  }
  try {
    if (await openPage(entry.url ?? searchAddress(entry.text))) {
      addressEditing = false;
      selection = "";
    }
  } catch (e) {
    error = errorText(e);
  }
}
function webLabel(entry: WebEntry): string {
  return t(entry.kind === "open" ? "web.openAddress" : "web.search", { text: entry.text });
}
let leftovers = $state<Leftover[]>([]);
let pinned = $state<Pinned>({ version: "", items: [], restorable: false });
/** Path of the one expanded card, or "". Not remembered when the panel closes. */
let expanded = $state("");
/** Chosen with the arrows or the mouse; "" means the default for the query. */
let selection = $state("");
/** Raw text of the notes shown as preview or expanded. */
let contents = $state<Record<string, OpenNote>>({});

// One card edits at a time, in place: the editor replaces the expanded card's content.
/** The note being edited; while editing it is also the expanded card. */
let selected = $state("");
/** Version of the edited note, to tell a real change from a reread of the same bytes. */
let version = "";
let opened = $state<OpenNote | null>(null);
let editing = $state(false);
let saveStatus = $state<AutosaveState>("saved");
let editor = $state<Editor>();
let leaving: Promise<boolean> | null = null;
/** The note the editor last left, under the name it left with: leaving may rename it. */
let leftPath = "";
export function leaveEditor(): Promise<boolean> {
  if (!editing) return Promise.resolve(true);
  if (leaving) return leaving;
  leaving = (async () => {
    // A title being typed is renamed first, while the editor still holds the note.
    const field = document.activeElement;
    if (field instanceof HTMLInputElement && field.closest(".card.editing .rename")) field.blur();
    await renamingTask;
    // An untitled note takes its name from what it shows, while the editor still holds it.
    const named = editor && isUntitled(selected) ? noteName(editor.text()) : null;
    if (named) await rename(named.title, named.frontmatter);
    const note = await editor?.finish();
    if (!note) return false;
    contents[selected] = note;
    leftPath = selected;
    selected = "";
    opened = note;
    version = note.version;
    editing = false;
    return true;
  })().finally(() => {
    leaving = null;
  });
  return leaving;
}
/** Note whose editor read is in flight; a rename of it cancels the open. */
let opening: string | null = null;
async function edit(path: string) {
  if (editing || choosing) return;
  const current = generation;
  opening = path;
  try {
    error = "";
    const note = await notesOpen(path);
    // A folder change or rename since the read: the note may belong to another folder now, or
    // saving under the old path would bring the renamed file back.
    if (editing || choosing || current !== generation || opening !== path) return;
    contents[path] = note;
    opened = note;
    version = note.version;
    selected = path;
    expanded = path;
    editing = true;
  } catch (e) {
    error = errorText(e);
  } finally {
    if (opening === path) opening = null;
  }
}
/**
 * Leaving by keyboard (Esc or Ctrl+E): the card stays expanded and the list takes focus, so the
 * next Esc reaches the list rules instead of closing the panel.
 */
async function leaveByKeyboard() {
  if (!(await leaveEditor())) return;
  await tick();
  const focused = document.activeElement;
  if (focused && focused !== document.body) return;
  list?.focus();
}
/**
 * A mouse click leaves focus on an untabbable button inside the list. The list keys then move the
 * selection, but the button would keep focus and turn on its keyboard focus ring, so the list takes
 * focus back. Handlers that moved focus elsewhere (the editor) are left alone.
 */
function returnFocus(event: MouseEvent) {
  const focused = document.activeElement;
  if (event.detail === 0 || !(focused instanceof HTMLButtonElement) || focused.tabIndex >= 0)
    return;
  if (list?.contains(focused)) list.focus({ preventScroll: true });
}
const saveText = $derived(
  saveStatus === "saved"
    ? t("editor.saved")
    : saveStatus === "saving" || saveStatus === "pending"
      ? t("editor.saving")
      : "",
);

/**
 * Bumped by every action that replaces what is shown: choosing a folder or renaming the open note. A
 * read that started before the bump answers for an old folder or note and is dropped.
 */
let generation = 0;

// Search: the index follows the listing, one sync at a time, and starts over with each folder.
const folderIndex = new FolderIndex(notesTexts);
let indexVersion = $state(0);
async function sync(list: Note[]): Promise<void> {
  await folderIndex.sync(list);
  indexVersion++;
}

const results = $derived.by(() => {
  void indexVersion;
  return search(folderIndex.index, query);
});
const kept = $derived.by(() => {
  void indexVersion;
  if (!expanded || results.hits.some((hit) => hit.path === expanded)) return null;
  const listed = notes.some((note) => note.path === expanded);
  if (!listed && !contents[expanded]) return null;
  const name = expanded.slice(expanded.lastIndexOf("/") + 1).replace(/\.md$/i, "");
  const entry = listed ? folderIndex.index.get(expanded) : undefined;
  return {
    path: expanded,
    title: entry?.title ?? name,
    tags: entry?.tags ?? [],
    url: entry?.url,
    modified: entry?.modified ?? 0,
    snippet: null,
    gone: !listed,
  };
});
const model = $derived(
  buildList({
    hits: orderHits(results.hits, notes, $noteOrder, query),
    exactTitle: results.exactTitle,
    query,
    pinned: pinned.items,
    expanded,
    kept,
    editing,
  }),
);
let groupingDate = $state(new Date());
const headings = $derived(
  dateHeadings(
    model.notes.map((card) => card.hit),
    notes,
    $noteOrder,
    $groupByDate,
    query,
    groupingDate,
  ),
);
const previewModel = $derived.by(() => {
  const source = dragged?.target;
  if (!source || !dragOver || pinned.version !== dragged?.base) return model;
  const entries = [...model.pinned];
  const from = entries.findIndex((entry) => entryKey(entry) === source);
  const to = entries.findIndex((entry) => entryKey(entry) === dragOver);
  if (from < 0 || to < 0) return model;
  entries.splice(to, 0, ...entries.splice(from, 1));
  return { ...model, pinned: entries };
});
const listRows = $derived(rows(previewModel, editing ? selected : "", headings));
const noteKeys = $derived(browser || intent !== null || !folder?.available ? [] : navKeys(model));
const shownWebItems = $derived(
  showingHistory ? historyEntries(history) : browser && !addressEditing ? [] : webItems,
);
const keys = $derived([...noteKeys, ...shownWebItems.map((item) => item.key)]);
const active = $derived(
  keys.includes(selection)
    ? selection
    : barDefault(
        inputQuery,
        browser,
        folder?.available ? defaultKey(model, query) : "",
        shownWebItems,
      ),
);

/** What a screen reader hears when the selection changes. */
const announced = $derived.by(() => {
  const webEntry = shownWebItems.find((entry) => entry.key === active);
  if (webEntry) return webLabel(webEntry);
  if (active === "create" && model.create !== null) return t("list.create", { text: model.create });
  const entry = [...model.pinned, ...model.notes].find((e) => entryKey(e) === active);
  if (!entry) return "";
  return isMissing(entry) ? `${entry.title}, ${t("card.missing")}` : entry.hit.title;
});

function cardFor(path: string): Card | undefined {
  for (const entry of [...model.pinned, ...model.notes])
    if (!isMissing(entry) && entry.hit.path === path) return entry;
}

// Preview and expanded cards need their text.
const wanted = $derived([
  ...pinned.items
    .filter((i) => i.kind === "note" && !i.missing && !i.collapsed)
    .map((i) => i.target),
  ...(expanded ? [expanded] : []),
]);
const loading = new Set<string>();
async function load(path: string) {
  const current = generation;
  loading.add(path);
  try {
    const note = await notesOpen(path);
    if (current === generation) contents[path] = note;
  } catch (e) {
    if (current === generation) error = errorText(e);
  } finally {
    loading.delete(path);
  }
}
function loadWanted() {
  for (const path of wanted) if (!(path in contents) && !loading.has(path)) void load(path);
}
$effect(() => {
  void wanted;
  untrack(loadWanted);
});

async function reloadContents(current: number) {
  const listed = new Set(notes.map((note) => note.path));
  const keep = new Set([...wanted, selected]);
  for (const path of Object.keys(contents)) if (!keep.has(path)) delete contents[path];
  await Promise.all(
    Object.keys(contents)
      .filter((path) => path !== selected && listed.has(path))
      .map(async (path) => {
        try {
          const note = await notesOpen(path);
          if (current === generation && note.version !== contents[path]?.version)
            contents[path] = note;
        } catch {
          // Keeps the last content; the listing says if the note is gone.
        }
      }),
  );
}

async function refresh(value: NotesFolder | null) {
  const current = ++generation;
  if (editing) return;
  folder = value;
  notes = [];
  selected = "";
  opened = null;
  leftovers = [];
  contents = {};
  expanded = "";
  selection = "";
  pinned = { version: "", items: [], restorable: false };
  folderIndex.reset();
  indexVersion++;
  if (!value?.available) return;
  const list = await notesList();
  if (current !== generation) return;
  notes = list;
  await sync(list);
  if (current !== generation) return;
  const state = await pinnedList();
  if (current !== generation) return;
  pinned = state;
  const found = await notesLeftovers();
  if (current === generation) leftovers = found;
}

async function restore(path: string) {
  try {
    error = "";
    if (!(await leaveEditor())) return;
    const note = await notesRestore(path);
    leftovers = await notesLeftovers();
    const list = await notesList();
    notes = list;
    await sync(list);
    expanded = note;
    selection = note;
  } catch (e) {
    error = errorText(e);
  }
}

/** While the folder picker is open no editor can open: its note would belong to the old folder. */
let choosing = false;
export async function choose() {
  if (choosing || savingPage) return;
  choosing = true;
  try {
    error = "";
    if (!(await leaveEditor())) return;
    await refresh(await notesChoose());
  } catch (e) {
    error = errorText(e);
    return error;
  } finally {
    choosing = false;
  }
}

/** Creates the note for the typed text and opens it expanded. */
async function create(text: string, content = "", annotate = false) {
  try {
    error = "";
    if (!(await leaveEditor())) return;
    const taken = notes.map((note) => note.path);
    // Another app may have taken the name since the listing: the save then conflicts, and the next name is tried.
    for (let attempt = 0; attempt < 50; attempt++) {
      const name = noteFileName(text, taken);
      const result = await notesSave(name, content, "");
      if (result.status === "saved") {
        const list = await notesList();
        notes = list;
        await sync(list);
        query = "";
        expanded = name;
        selection = name;
        if (annotate) {
          await webSetVisible(false);
          await edit(name);
        }
        return;
      }
      taken.push(name);
    }
    error = errorText({ code: "saveFailed", detail: text });
  } catch (e) {
    error = errorText(e);
  }
}

let savingPage = $state(false);
let pageUnavailable = $state(false);
$effect(() => {
  void pageUrl;
  void browser;
  pageUnavailable = false;
});
async function savePage() {
  if (savingPage || choosing) return;
  savingPage = true;
  try {
    error = "";
    const page = await webSelection();
    pageUnavailable = page === null;
    if (!page) return;
    const note = pageNote(page);
    await create(note.name, note.text, true);
  } catch (e) {
    error = errorText(e);
  } finally {
    savingPage = false;
  }
}

/** Moves a card to `level`; for a pinned card, line against open is also remembered. */
async function setLevel(card: Card, level: Level) {
  if (card.url) return;
  let path = card.hit.path;
  try {
    error = "";
    // Closing the edited card or expanding another leaves editing first: it saves, or refuses on a
    // conflict or error and then nothing changes.
    if (editing && (path !== selected || level !== "expanded")) {
      const edited = path === selected;
      if (!(await leaveEditor())) return;
      if (edited) path = leftPath;
    }
    selection = path;
    if (level === "expanded") expanded = path;
    else if (expanded === path) expanded = "";
    if (level !== "line") reveal(path);
    if (!card.pinned) return;
    const item = pinned.items.find((i) => i.target === path);
    const collapsed = level === "line";
    if (item && item.collapsed !== collapsed) pinned = await pinnedSetCollapsed(path, collapsed);
  } catch (e) {
    error = errorText(e);
  }
}

function toggle(card: Card) {
  if (card.url) {
    void openPage(card.url).catch((e) => {
      error = errorText(e);
    });
    return;
  }
  const level = card.level === "line" ? deeper(card) : "line";
  if (level) void setLevel(card, level);
}

/** Opened cards scroll into view, quickly, while their content renders and grows. */
let stopReveal: (() => void) | null = null;
function reveal(path: string) {
  stopReveal?.();
  const view = list;
  const card = view?.querySelector<HTMLElement>(`.card[data-path="${CSS.escape(path)}"]`);
  if (!view || !card) return;
  const scroll = () => {
    // Positions without the move animation, which shifts the entry while it plays.
    const entry = card.closest(".list-entry");
    const shift = entry ? new DOMMatrix(getComputedStyle(entry).transform).m42 : 0;
    const rect = card.getBoundingClientRect();
    const delta = revealScroll(
      { top: rect.top - shift, bottom: rect.bottom - shift },
      view.getBoundingClientRect(),
    );
    if (delta) scrollListTo(view, view.scrollTop + delta);
  };
  const observer = new ResizeObserver(scroll);
  observer.observe(card);
  const timer = window.setTimeout(stop, 600);
  function stop() {
    observer.disconnect();
    window.clearTimeout(timer);
    stopReveal = null;
  }
  stopReveal = stop;
}
let scrollFrame = 0;
function scrollListTo(view: HTMLElement, top: number) {
  cancelAnimationFrame(scrollFrame);
  const start = view.scrollTop;
  const duration = 2 * animationDuration;
  if (!duration) {
    view.scrollTop = top;
    return;
  }
  const begin = performance.now();
  const step = (now: number) => {
    const progress = Math.min(1, (now - begin) / duration);
    view.scrollTop = start + (top - start) * cubicOut(progress);
    if (progress < 1) scrollFrame = requestAnimationFrame(step);
  };
  scrollFrame = requestAnimationFrame(step);
}

/**
 * Moves list entries to their new place by translation only. Svelte's `flip` also scales an entry
 * whose size changed, so an opening card grew from its middle; it grows from its top edge instead.
 */
function slide(_node: HTMLElement, { from, to }: { from: DOMRect; to: DOMRect }) {
  const dx = from.left - to.left;
  const dy = from.top - to.top;
  if (!dx && !dy) return { duration: 0 };
  return {
    duration: animationDuration,
    easing: cubicOut,
    css: (_t: number, u: number) => `transform: translate(${u * dx}px, ${u * dy}px)`,
  };
}

let pinning = false;
/** `refocus`: the toggle came from the editor, which can lose focus when its card changes section. */
async function pinOrUnpin(card: Card, refocus = false) {
  if (pinning) return;
  pinning = true;
  selection = card.hit.path;
  try {
    error = "";
    pinned = card.pinned
      ? await pinnedRemove(card.hit.path)
      : await pinnedAdd(card.hit.path, card.hit.title);
    if (refocus) {
      await tick();
      editor?.focus();
    }
  } catch (e) {
    error = errorText(e);
  } finally {
    pinning = false;
  }
}

/** The renames the title field started, in order; leaving the editor waits for them. */
let renamingTask: Promise<void> | null = null;
/**
 * The title field of the edited card was left with `title`. The file name comes from it, in the same
 * folder; a frontmatter title, which the card shows, changes with it as an edit of the text. The
 * editor stays open: it saves first and holds later saves until the move, so none brings the old name
 * back. A taken name is retried with the next free one. A rename whose pinned items could not follow
 * still stands. A rename asked while another runs waits for it and starts from its result. With
 * `keepTitle`, the frontmatter title stays as it is and the file follows it.
 */
function rename(title: string, keepTitle = false): Promise<void> {
  const typed = title.replace(/\s+/g, " ").trim();
  const previous = renamingTask;
  const task = (async () => {
    await previous;
    const card = editing ? cardFor(selected) : undefined;
    const current = editor;
    if (!typed || !card || !current || (!keepTitle && typed === card.hit.title)) return;
    const from = card.hit.path;
    const old = card.hit.title;
    try {
      error = "";
      await current.rename(
        (text) => (keepTitle ? null : retitle(text, typed)),
        async (version, retitled) => {
          const taken = notes.map((note) => note.path);
          for (let attempt = 0; attempt < 50; attempt++) {
            const to = renamedPath(from, typed, taken);
            if (to === from) return;
            const shown = keepTitle || retitled ? typed : to.slice(to.lastIndexOf("/") + 1, -3);
            try {
              await notesRename(from, to, version, [old, shown]);
              changed({ renames: [[from, to]] });
              return;
            } catch (e) {
              const code = (e as { code?: string }).code;
              if (code === "pinnedNotRenamed") {
                changed({ renames: [[from, to]] });
                error = errorText(e);
                return;
              }
              if (code !== "renameConflict") throw e;
            }
            taken.push(to);
          }
          throw { code: "saveFailed", detail: typed };
        },
      );
    } catch (e) {
      error = errorText(e);
    }
  })();
  renamingTask = task;
  void task.finally(() => {
    if (renamingTask === task) renamingTask = null;
  });
  return task;
}

/** F2: the title field of the note's editor, opening it if needed, with the title selected. */
async function editTitle(path: string) {
  if (!(editing && selected === path)) {
    if (!(await leaveEditor())) return;
    await edit(path);
  }
  await tick();
  const input = list?.querySelector<HTMLInputElement>(".card.editing .rename input");
  input?.focus();
  input?.select();
}

let reordering = false;
let dragged = $state<{ target: string; base: string } | null>(null);
let dragOver = $state("");
let dragImageCaptured = $state(false);
let animationDuration = $state(0);
let dropping = false;
let dragLayout: { keys: string[]; heights: number[]; top: number; scroll: number } | null = null;
// Moving a row in the DOM drops the focus inside it, as when a renamed note takes its new place in
// the order: the edited card's focus, in its title field or editor, comes back after the move.
let movedFocus: HTMLElement | null = null;
$effect.pre(() => {
  void listRows;
  const focused = document.activeElement;
  movedFocus = focused instanceof HTMLElement && focused.closest(".card.editing") ? focused : null;
});
$effect(() => {
  void listRows;
  const focused = movedFocus;
  movedFocus = null;
  if (!focused?.isConnected || document.activeElement !== document.body) return;
  // The editor lives in a shadow root, whose host is what the document reports as focused.
  if (focused.classList.contains("surface")) editor?.focus();
  else focused.focus({ preventScroll: true });
});
function overPin(event: DragEvent) {
  if (!dragged || !dragLayout || editing || !list) return;
  const { keys, heights, top, scroll } = dragLayout;
  const y = event.clientY - top + list.scrollTop - scroll;
  if (y < 0 || y > heights.reduce((sum, height) => sum + height, 0)) return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  if (event.type === "drop") {
    void dropPin(event);
    return;
  }
  const next = keys[dragSlot(heights, keys.indexOf(dragged.target), keys.indexOf(dragOver), y)];
  if (next !== dragOver) dragOver = next;
}
onMount(() => {
  const media = matchMedia("(prefers-reduced-motion: reduce)");
  const update = () => {
    animationDuration = media.matches
      ? 0
      : Number.parseFloat(
          getComputedStyle(document.documentElement).getPropertyValue("--duration"),
        );
  };
  update();
  media.addEventListener("change", update);
  return () => media.removeEventListener("change", update);
});
function dragEnd() {
  if (dropping) return;
  dragImageCaptured = false;
  dragLayout = null;
  dragged = null;
  dragOver = "";
}
async function dropPin(event: DragEvent) {
  if (!dragged) return;
  event.preventDefault();
  dropping = true;
  await reorder(dragged.target, dragOver || dragged.target, dragged.base);
  dropping = false;
  dragEnd();
}
async function reorder(target: string, to: string, base = pinned.version) {
  if (reordering || editing || target === to) return;
  reordering = true;
  try {
    error = "";
    pinned = await pinnedMove(target, to, base);
    selection = target;
  } catch (e) {
    error = errorText(e);
  } finally {
    reordering = false;
  }
}
/** Only headers drag, preserving native note text selection. */
function pinDrag(target: string) {
  return (element: HTMLElement) => {
    const handle = element.querySelector<HTMLElement>(".head") ?? element;
    handle.draggable = !editing;
    const start = (event: DragEvent) => {
      if (editing || reordering || !list) {
        event.preventDefault();
        return;
      }
      const rect = element.getBoundingClientRect();
      event.dataTransfer?.setDragImage(
        element,
        event.clientX - rect.left,
        event.clientY - rect.top,
      );
      const slots = [...list.querySelectorAll<HTMLElement>(".list-entry[data-pin]")];
      dragLayout = {
        keys: slots.map((slot) => slot.dataset.pin ?? ""),
        heights: slots.map((slot) => slot.getBoundingClientRect().height),
        top: slots[0].getBoundingClientRect().top,
        scroll: list.scrollTop,
      };
      dragged = { target, base: pinned.version };
      dragOver = target;
      requestAnimationFrame(() => {
        if (dragged) dragImageCaptured = true;
      });
      event.dataTransfer?.setData("text/plain", target);
      if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
    };
    handle.addEventListener("dragstart", start);
    handle.addEventListener("dragend", dragEnd);
    return () => {
      handle.draggable = false;
      handle.removeEventListener("dragstart", start);
      handle.removeEventListener("dragend", dragEnd);
    };
  };
}
async function pinnedAction(action: () => Promise<Pinned>) {
  try {
    error = "";
    pinned = await action();
  } catch (e) {
    error = errorText(e);
  }
}

/** Removes a pinned item whose note is gone; the selection moves to the next item, else the previous. */
async function removeMissing(target: string) {
  const at = keys.indexOf(target);
  await pinnedAction(() => pinnedRemove(target));
  if (selection === target) selection = keys[at] ?? keys[at - 1] ?? "";
}

/**
 * Rereads after a change on disk. The edited note is replaced only if its bytes changed, so a save of
 * the same text does not disturb it; a note that disappeared keeps its last content on screen.
 */
async function reload() {
  const current = generation;
  const editRevision = editor?.snapshot();
  // A newer action made this read stale; reading again picks up the change for what is shown now.
  const stale = () => {
    if (current !== generation) again = true;
    return current !== generation;
  };
  const value = await notesFolder();
  if (stale()) return;
  if (!value?.available) {
    if (editing) {
      editor?.unavailable({ code: "folderUnavailable" });
      return;
    }
    return refresh(value);
  }
  const list = await notesList();
  if (stale()) return;
  notes = list;
  await sync(list);
  if (stale()) return;
  const state = await pinnedList();
  if (stale()) return;
  pinned = state;
  await reloadContents(current);
  loadWanted();
  if (stale()) return;
  const path = selected;
  if (!path) return;
  if (!notes.some((note) => note.path === path)) {
    if (editing) editor?.missing();
    return;
  }
  const note = await notesOpen(path);
  if (stale()) return;
  if (note.version !== version) {
    if (editing && !editor?.reload(note, editRevision)) return;
    opened = note;
    version = note.version;
  }
}

let reloading: Promise<void> | null = null;
let again = false;

/** One reload at a time; changes during it cause one more. */
function changed(changes: NotesChanges) {
  for (const [from, to] of changes.renames) {
    if (opening === from) opening = null;
    if (from in contents) {
      contents[to] = contents[from];
      delete contents[from];
    }
    if (expanded === from) expanded = to;
    if (selection === from) selection = to;
    if (selected !== from) continue;
    selected = to;
    editor?.moved(to);
    generation++;
  }
  if (reloading) {
    again = true;
    return;
  }
  reloading = (async () => {
    do {
      again = false;
      try {
        await reload();
      } catch (e) {
        if (editing) editor?.unavailable(e);
        else error = errorText(e);
      }
    } while (again);
    reloading = null;
  })();
}

/** Leave editing through its save guard before replacing the list. */
async function openPage(url: string) {
  if (!(await leaveEditor())) return false;
  await webOpen(url);
  query = "";
  return true;
}

/** A click on a link in a card's content; true if it was one. */
function linkClick(event: MouseEvent): boolean {
  const link = (event.target as Element).closest("a");
  if (!link) return false;
  event.preventDefault();
  const href = link.getAttribute("href") ?? "";
  if (href.startsWith("#")) {
    // The sanitizer only adds the prefix when it is not already present.
    const id = href.slice(1);
    document.getElementById(id.startsWith("note-") ? id : `note-${id}`)?.scrollIntoView();
  } else if (/^(https?:|mailto:)/i.test(href))
    void (event.ctrlKey || /^mailto:/i.test(href) ? openExternal(href) : openPage(href)).catch(
      (e) => {
        error = errorText(e);
      },
    );
  return true;
}

let tools = $state<HTMLElement>();
let list = $state<HTMLElement>();
const bar = () => tools?.querySelector("input");
export function focusSearch() {
  void tick().then(() => {
    bar()?.focus();
    bar()?.select();
  });
}

/** Typing searches: the edited card is left the guarded way and stays expanded, reading. */
function typed() {
  showingHistory = false;
  if (browser) addressEditing = true;
  selection = "";
  void leaveEditor();
}

/** About three lines of the list's text: one ↑ or ↓ step inside a long expanded card. */
function lineStep(element: HTMLElement): number {
  return 3 * (Number.parseFloat(getComputedStyle(element).lineHeight) || 20);
}

/** ↑ / ↓ scroll a long expanded card before leaving it; with Alt they always move the selection. */
function arrow(event: KeyboardEvent) {
  const down = event.key === "ArrowDown";
  const card = cardFor(active);
  if (!event.altKey && list && card?.level === "expanded") {
    const element = list.querySelector(`.card[data-path="${CSS.escape(card.hit.path)}"]`);
    if (element) {
      const delta = arrowScroll(
        element.getBoundingClientRect(),
        list.getBoundingClientRect(),
        down,
        lineStep(list),
      );
      if (delta) {
        list.scrollBy({ top: delta });
        return;
      }
    }
  }
  selection = move(keys, active, down ? 1 : -1);
}

/** An explicit close leaves editing first, the guarded way; on a conflict or error the panel stays. */
async function closePanel() {
  if (await leaveEditor()) await panelClose();
}

/**
 * Esc, in layers: leave editing; clear the search and return to the bar; from the empty bar, close
 * the panel. A pinned panel stays.
 */
function escapeKey(inBar: boolean) {
  if (showingHistory) {
    showingHistory = false;
    addressEditing = false;
    selection = "";
    return;
  }
  if (editing && !inBar) {
    void leaveByKeyboard();
    return;
  }
  const input = bar();
  if (browser) {
    if (addressEditing) {
      addressEditing = false;
      address = pageUrl;
      selection = "";
      input?.focus();
    } else void closePanel();
    return;
  }
  if (query || (input && !inBar)) {
    query = "";
    selection = "";
    input?.focus();
    return;
  }
  void closePanel();
}

/** List keys, from the search bar (`inBar`) or from anywhere in the panel outside a text field. */
function listKey(event: KeyboardEvent, inBar: boolean) {
  if (!list && !shownWebItems.length && event.key !== "Escape") return;
  const modified = event.ctrlKey || event.shiftKey || event.metaKey;
  switch (event.key) {
    case "Escape":
      // Before the field's own Esc, which would leave it.
      event.preventDefault();
      event.stopPropagation();
      escapeKey(inBar);
      return;
    case "ArrowDown":
    case "ArrowUp":
      if (
        event.altKey &&
        event.shiftKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        !browser &&
        !editing
      ) {
        const entries = model.pinned.map(entryKey);
        const at = entries.indexOf(active);
        const to = entries[at + (event.key === "ArrowUp" ? -1 : 1)];
        if (at >= 0) {
          event.preventDefault();
          if (to) void reorder(active, to);
        }
        return;
      }
      if (modified) return;
      event.preventDefault();
      arrow(event);
      return;
    case "PageDown":
    case "PageUp":
      if (modified || event.altKey || !list) return;
      event.preventDefault();
      list.scrollBy({ top: (event.key === "PageDown" ? 0.9 : -0.9) * list.clientHeight });
      return;
    case "ArrowRight":
    case "ArrowLeft": {
      if (modified || event.altKey) return;
      // In the bar the arrows move the caret; they open and close only from its natural place, the end.
      if (inBar && !caretAtEnd(event)) return;
      const card = cardFor(active);
      const level = card && (event.key === "ArrowRight" ? deeper(card) : shallower(card));
      if (!card || !level) return;
      event.preventDefault();
      void setLevel(card, level);
      return;
    }
    case "Enter": {
      event.preventDefault();
      if (event.ctrlKey) {
        if (!browser && intent === null && folder?.available && query.trim())
          void create(query.trim());
        return;
      }
      const webEntry = shownWebItems.find((entry) => entry.key === active);
      if (webEntry) {
        void activateWeb(webEntry);
        return;
      }
      const action = enter(model, active);
      if (action?.action === "create") void create(action.text);
      else if (action?.action === "remove") void removeMissing(action.target);
      else if (action?.action === "openPage") {
        void openPage(action.url).catch((e) => {
          error = errorText(e);
        });
      } else if (action) {
        const card = cardFor(action.path);
        if (card) void setLevel(card, "expanded");
      }
      return;
    }
    case "Delete": {
      const action = enter(model, active);
      if (modified || event.altKey || action?.action !== "remove") return;
      if (inBar && !caretAtEnd(event)) return;
      event.preventDefault();
      void removeMissing(action.target);
      return;
    }
  }
  // Type to search: the key lands in the bar once it has the focus. Space stays with buttons.
  const printable = event.key.length === 1 && event.key !== " ";
  if (!inBar && printable && !event.ctrlKey && !event.altKey && !event.metaKey) bar()?.focus();
}

/** The key's field has its caret at the end with nothing selected, where the key edits nothing. */
function caretAtEnd(event: KeyboardEvent): boolean {
  const input = event.target;
  return (
    !(input instanceof HTMLInputElement) ||
    (input.selectionStart === input.selectionEnd && input.selectionEnd === input.value.length)
  );
}

/** The search bar's keys. Runs in capture, before the field's own Esc. */
function barKey(event: KeyboardEvent) {
  if (!(event.target instanceof HTMLInputElement) || event.isComposing) return;
  if (
    event.key === "ArrowDown" &&
    !event.ctrlKey &&
    !event.altKey &&
    !event.shiftKey &&
    !event.metaKey &&
    downOpensHistory(browser, shownWebItems.length)
  ) {
    // Without history, ↓ keeps moving through the notes.
    const first = historyEntries(history)[0];
    if (first) {
      event.preventDefault();
      showingHistory = true;
      addressEditing = browser;
      selection = first.key;
      return;
    }
  }
  listKey(event, true);
}

/** A text field or the editor keeps its keys; the editor sits in a shadow root. */
function inField(event: KeyboardEvent): boolean {
  const target = event.composedPath()[0];
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || target.matches("input, textarea, select"))
  );
}

function shortcut(event: KeyboardEvent) {
  if (inactive || event.defaultPrevented || event.isComposing) return;
  if (
    browser &&
    !tools?.contains(event.target as Node) &&
    !(event.ctrlKey && event.key.toLowerCase() === "f")
  ) {
    if (event.key === "Escape") {
      event.preventDefault();
      void panelClose();
    }
    return;
  }
  const input = bar();
  const inBar = input !== undefined && input !== null && event.target === input;
  const field = !inBar && inField(event);
  const ctrl = event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey;
  const key = event.key.toLowerCase();
  // Cycle the three areas without entering card controls or leaving the WebView.
  if (
    key === "tab" &&
    (!field || (event.target instanceof HTMLElement && event.target.closest(".note-order"))) &&
    !event.ctrlKey &&
    !event.altKey &&
    !event.metaKey &&
    list &&
    input
  ) {
    // Recovery actions (leftovers, a deleted pinned list) sit between the bar and the cards.
    const stops = [
      input,
      ...document.querySelectorAll<HTMLElement>(".leftover button, .restore button"),
      ...(!browser ? [list, ...list.querySelectorAll<HTMLElement>(".note-order > button")] : []),
      ...document.querySelectorAll<HTMLElement>("main > header button"),
      ...(tools?.querySelectorAll<HTMLElement>("button") ?? []),
      ...document.querySelectorAll<HTMLElement>("main > .resize"),
    ];
    const target = event.target as HTMLElement;
    const own = stops.indexOf(target);
    const at = own >= 0 ? own : list.contains(target) ? stops.indexOf(list) : -1;
    if (at >= 0) {
      event.preventDefault();
      stops[(at + (event.shiftKey ? stops.length - 1 : 1)) % stops.length].focus();
      return;
    }
  }
  const inEditor =
    editing &&
    event
      .composedPath()
      .some((node) => node instanceof HTMLElement && node.classList.contains("cm-content"));
  if (!browser && ctrl && key === "d" && (!field || inEditor)) {
    const card = cardFor(inEditor ? selected : active);
    if (!card) return;
    event.preventDefault();
    // Held down, the key repeats: one press is one toggle.
    if (!event.repeat) void pinOrUnpin(card, inEditor);
    return;
  }
  if (!browser && ctrl && key === "e") {
    const path = editing ? selected : (cardFor(active)?.hit.path ?? expanded);
    if (!path || cardFor(path)?.url) return;
    event.preventDefault();
    if (editing) void leaveByKeyboard();
    else void edit(path);
    return;
  }
  const plain = !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey;
  if (!browser && plain && key === "f2" && (!field || inEditor)) {
    const path = inEditor ? selected : cardFor(active)?.hit.path;
    if (!path || cardFor(path)?.url) return;
    event.preventDefault();
    void editTitle(path);
    return;
  }
  if (ctrl && key === "f" && !field) {
    event.preventDefault();
    input?.focus();
    input?.select();
    return;
  }
  // The bar's keys ran in capture.
  if (!inBar && !field) listKey(event, false);
}

onMount(() => {
  // Refresh after midnight and after a suspended machine resumes, without remounting cards.
  const clock = window.setInterval(() => {
    groupingDate = new Date();
  }, 30_000);
  const unlisten = onNotesChanged(changed);
  // An expanded card returns to preview when the panel closes, after editing is left the guarded way.
  const hidden = onPanelHidden(() => {
    void leaveEditor().then((left) => {
      if (left) expanded = "";
    });
    // WebView2 keeps its inner focus after Windows moves the foreground away: without this, a
    // hover-opened panel would show a focused bar and caret while it has no focus.
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
  });
  // The shortcut goes to the search bar with its text selected, so typing replaces it; pressed with
  // the bar already focused, it closes the panel.
  const search = onPanelSearch((toggle) => {
    if (inactive) return;
    const input = bar();
    if (toggle && (!input || document.activeElement === input)) {
      void closePanel();
      return;
    }
    input?.focus();
    input?.select();
  });
  notesFolder()
    .then(refresh)
    .catch((e) => {
      error = errorText(e);
    });
  return () => {
    window.clearInterval(clock);
    void unlisten.then((stop) => stop());
    void hidden.then((stop) => stop());
    void search.then((stop) => stop());
  };
});
</script>

<svelte:window onkeydown={shortcut} onblur={() => { addressEditing = false; showingHistory = false; }} />

{#snippet noteEditor()}
  {#if opened}
    <Editor
      bind:this={editor}
      path={selected}
      note={opened}
      onstatus={(state) => {
        saveStatus = state;
      }}
      onleave={() => void leaveByKeyboard()}
      ondiscard={() => {
        void load(selected);
        selected = "";
        editing = false;
      }}
    />
  {/if}
{/snippet}

<section hidden={inactive} aria-label={t("notes.label")}>
    <div class="tools" bind:this={tools} onkeydowncapture={barKey} oninput={typed} role="presentation">
      <TextField
        value={inputQuery}
        oninput={(event) => { if (browser) address = event.currentTarget.value; else query = event.currentTarget.value; }}
        aria-controls="note-list"
        aria-label={t(browser ? "web.address" : "notes.search")}
        placeholder={t("notes.searchPlaceholder")}
      />
      <IconButton quiet icon="plus" label={t(browser ? "web.saveNote" : "notes.create")} disabled={!folder?.available || (browser && (!pageUrl || pageUnavailable || savingPage))} onclick={() => browser ? savePage() : create(query.trim(), "", true)} />
    </div>
  {#if browser && addressEditing && shownWebItems.length}
    <div class="web-suggestions" bind:this={suggestionsElement}>
      {#each shownWebItems as entry (entry.key)}
        <button class="suggestion" class:selected={active === entry.key} tabindex="-1" aria-current={active === entry.key || undefined} onclick={() => activateWeb(entry)}
          {@attach (element) => { if (active === entry.key) element.scrollIntoView({ block: "nearest" }); }} aria-label={webLabel(entry)}><Icon name={entry.key.startsWith("web:history:") ? "restore" : entry.kind === "open" ? "globe" : "search"} /><span>{entry.text}</span></button>
      {/each}
    </div>
  {/if}
  <p class="announce" aria-live="polite">{announced}</p>
  {#if browser && error}<p class="error" role="alert">{error}</p>{/if}
  {#if browser && !pageUrl}<div class="empty"><p>{t("web.empty")}</p></div>{/if}
  <div class="content" hidden={browser}>
  {#if folder?.available}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if leftovers.length > 0}
      <section class="leftovers" aria-label={t("leftovers.label")}>
        <p class="message">{t("leftovers.notice")}</p>
        {#each leftovers as leftover (leftover.path)}
          <div class="leftover" title={leftover.path}>
            <span>{leftover.note}</span>
            {#if leftover.modified}<small>{dateTime(leftover.modified)}</small>{/if}
            <IconButton
              icon="restore"
              label={t("leftovers.restore", { note: leftover.note })}
              onclick={() => restore(leftover.path)}
            />
          </div>
        {/each}
      </section>
    {/if}
    <!-- Plain structure: cards hold buttons, links and the editor, so they cannot be listbox options.
      The selected item carries aria-current and the live region announces it. -->

    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions: the list is one Tab stop driven by the list keys; its click only returns focus. -->
    <div class="list" id="note-list" role="region" aria-label={t("notes.label")}
      tabindex="0" bind:this={list} ondragover={overPin} ondrop={overPin} onclick={returnFocus}
      onfocus={(event) => { if (event.target === list && !active) selection = keys[0] ?? ""; }}>
      {#if pinned.restorable}
        <div class="restore">
          <p class="message">{t("pinned.deleted")}</p>
          <Button onclick={() => pinnedAction(pinnedRestore)}>{t("pinned.restore")}</Button>
        </div>
      {/if}
      {#if intent === null && model.create !== null}
        {@const text = model.create}
        <button tabindex="-1" aria-current={active === "create" || undefined} class="create" class:selected={active === "create"} onclick={() => create(text)}>
          {t("list.create", { text })}
        </button>
      {/if}
      <!-- One block for both sections, so a card that changes section moves instead of being rebuilt. -->
      {#each (intent === null ? listRows : []) as row (row.key)}
        <div class="list-entry" data-pin={"entry" in row && previewModel.pinned.includes(row.entry) ? entryKey(row.entry) : undefined} class:drag-gap={dragImageCaptured && dragged && "entry" in row && entryKey(row.entry) === dragged.target} animate:slide>
        {#if "heading" in row}
          <div class="section-heading"><h2>{t(row.heading === "pinned" ? "list.pinned" : "list.notes")}</h2>{#if row.heading === "notes"}<NoteOrder />{/if}</div>
        {:else if "dateGroup" in row}
          <h2>{row.dateGroup.kind === "calendarMonth"
            ? new Intl.DateTimeFormat(undefined, { month: "long" }).format(row.dateGroup.date)
            : row.dateGroup.kind === "year" ? new Date(row.dateGroup.date).getFullYear()
            : t(`group.${row.dateGroup.kind}`)}</h2>
        {:else if isMissing(row.entry)}
          {@const entry = row.entry}
          {@const current = active === entry.target}
          <div
            {@attach pinDrag(entry.target)}
            class="missing"
            class:selected={current}
            role="group"
            aria-label={entry.title}
            aria-current={current || undefined}
            {@attach (element) => {
              if (current) element.scrollIntoView({ block: "nearest" });
            }}
          >
            <span class="title" title={entry.target}>{entry.title}</span>
            <small>{t("card.missing")}</small>
            <IconButton
              tabindex={-1}
              quiet
              icon="x"
              label={t("card.remove")}
              onclick={() => removeMissing(entry.target)}
            />
          </div>
        {:else}
          {@const card = row.entry}
          {@const edited = editing && card.hit.path === selected}
          <CardView
            {card}
            drag={card.pinned ? pinDrag(card.hit.path) : undefined}
            text={contents[card.hit.path]?.text}
            selected={active === card.hit.path}
            onheader={() => toggle(card)}
            onrename={(title) => rename(title)}
            ontitledone={() => editor?.focus()}
            onexpand={() => setLevel(card, "expanded")}
            onpreview={() => setLevel(card, "preview")}
            onpin={() => pinOrUnpin(card)}
            onedit={() => (edited ? leaveEditor() : edit(card.hit.path))}
            onlink={linkClick}
            onpage={(url) => { void openPage(url).catch((e) => { error = errorText(e); }); }}
            editing={edited}
            status={edited ? saveText : ""}
            editor={edited ? noteEditor : undefined}
          />
        {/if}
        </div>
      {/each}
      {#each shownWebItems as entry (entry.key)}
        <button class="suggestion" class:selected={active === entry.key} tabindex="-1" aria-current={active === entry.key || undefined} onclick={() => activateWeb(entry)}
          {@attach (element) => { if (active === entry.key) element.scrollIntoView({ block: "nearest" }); }} aria-label={webLabel(entry)}><Icon name={entry.key.startsWith("web:history:") ? "restore" : entry.kind === "open" ? "globe" : "search"} /><span>{entry.text}</span></button>
      {/each}
      {#if !model.pinned.length && !notes.some((note) => !isPinnedList(note.path)) && !query.trim()}<p class="message">{t("notes.emptyFolder")}</p>{/if}
    </div>
  {:else}
    {#each shownWebItems as entry (entry.key)}
      <button class="suggestion" class:selected={active === entry.key} tabindex="-1" onclick={() => activateWeb(entry)}
          {@attach (element) => { if (active === entry.key) element.scrollIntoView({ block: "nearest" }); }} aria-label={webLabel(entry)}><Icon name={entry.key.startsWith("web:history:") ? "restore" : entry.kind === "open" ? "globe" : "search"} /><span>{entry.text}</span></button>
    {/each}
    <div class="empty">
      <p>{folder ? t("notes.unavailable") : t("notes.chooseHint")}</p>
      {#if folder}<p class="path">{folder.path}</p>{/if}
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <Button onclick={choose}>{t("notes.choose")}</Button>
    </div>
  {/if}
  </div>
</section>

<style>
  .web-suggestions {
    position: absolute;
    z-index: 1;
    top: var(--size-bar);
    left: var(--space-1);
    right: var(--space-1);
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-line);
    max-height: 40vh;
    overflow-y: auto;
  }
  .content {
    display: contents;
  }

  .content[hidden] {
    display: none;
  }
  section[hidden] { display: none; }
  section {
    position: relative;
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }

  .list {
    flex: 1;
    padding-bottom: var(--space-2);
    overflow-y: auto;
  }

  .list:focus-visible {
    outline: none;
  }

  .section-heading { display: flex; align-items: baseline; gap: var(--space-2); padding-right: var(--panel-inset-right); }
  .list-entry { display: flow-root; }
  .section-heading h2 { color: var(--color-fg); font-weight: 600; text-transform: uppercase; letter-spacing: calc(-3 * var(--tracking-tight)); }
  .drag-gap :global(.card), .drag-gap .missing { border-style: dashed; border-color: var(--color-muted); background: transparent; }
  .drag-gap :global(.card > *), .drag-gap .missing > * { visibility: hidden; }
  h2 {
    margin: 0;
    padding: var(--space-2) var(--panel-inset-right) var(--space-1) var(--panel-inset-left);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    font-weight: 400;
  }

  .create {
    display: block;
    box-sizing: border-box;
    width: calc(100% - var(--panel-inset-left) - var(--panel-inset-right));
    min-height: var(--size-control);
    margin: 0 var(--panel-inset-right) var(--space-2) var(--panel-inset-left);
    padding: var(--space-1) var(--space-2);
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-accent);
    font: inherit;
    font-size: var(--font-size-sm);
    font-weight: 600;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .create:hover,
  .create.selected {
    background: var(--color-selected);
  }

  .create.selected {
    border-color: var(--color-accent);
  }

  .suggestion {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: var(--size-control);
    padding: var(--space-1) var(--panel-inset-right) var(--space-1) var(--panel-inset-left);
    border: 0;
    background: var(--color-surface);
    color: var(--color-fg);
    font: inherit;
    font-size: var(--font-size-sm);
    text-align: left;
  }
  .suggestion.selected { background: var(--color-selected); }
  .suggestion span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .missing {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin: 0 var(--panel-inset-right) var(--space-2) var(--panel-inset-left);
    /* The title starts where card titles do: header padding plus title padding. */
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-3);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
  }

  .missing .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--color-muted);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .missing small {
    flex: none;
    color: var(--color-muted);
  }

  /* Like a selected line. */
  .missing.selected {
    border-color: var(--color-accent);
    background: var(--color-selected);
  }

  .missing.selected .title {
    color: var(--color-accent);
  }

  /* Heard, not seen. */
  .announce {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .restore {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    justify-content: space-between;
    padding: var(--space-1) var(--panel-inset-right) var(--space-1) var(--panel-inset-left);
  }

  .tools {
    display: flex;
    flex: none;
    gap: var(--space-2);
    box-sizing: border-box;
    height: var(--size-bar);
    align-items: center;
    padding: 0 var(--panel-inset-right) 0 var(--panel-inset-left);
  }

  .leftovers {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: var(--space-1);
    padding-bottom: var(--space-2);
    border-bottom: 1px solid var(--color-line);
  }

  .leftover {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    padding: 0 var(--panel-inset-right) 0 var(--panel-inset-left);
    font-size: var(--font-size-sm);
  }

  .leftover span {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .leftover small {
    flex: none;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }

  .message,
  .error {
    margin: 0;
    padding: var(--space-1) var(--panel-inset-right) var(--space-1) var(--panel-inset-left);
    font-size: var(--font-size-sm);
    overflow-wrap: anywhere;
  }

  .restore .message {
    padding: 0;
  }

  .message {
    color: var(--color-muted);
  }

  .error {
    color: var(--color-danger);
  }

  .empty {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    align-items: flex-start;
    padding: var(--space-4) var(--panel-inset-right) var(--space-4) var(--panel-inset-left);
  }

  .empty p {
    margin: 0;
    padding: 0;
  }

  .path {
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    overflow-wrap: anywhere;
  }
</style>
