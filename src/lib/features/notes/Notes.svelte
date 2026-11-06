<script lang="ts">
import { onMount, tick, untrack } from "svelte";
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
  pinnedRemove,
  pinnedRestore,
  pinnedSetCollapsed,
} from "../../ipc";
import Button from "../../ui/Button.svelte";
import IconButton from "../../ui/IconButton.svelte";
import TextField from "../../ui/TextField.svelte";
import type { AutosaveState } from "./autosave";
import CardView from "./Card.svelte";
import Editor from "./Editor.svelte";
import {
  arrowScroll,
  buildList,
  type Card,
  deeper,
  defaultKey,
  enter,
  isMissing,
  type Level,
  move,
  navKeys,
  noteFileName,
  rows,
  shallower,
} from "./list";
import { type SearchIndex, search, syncIndex } from "./search";

let folder = $state<NotesFolder | null>(null);
let notes = $state<Note[]>([]);
let error = $state("");
let query = $state("");
let leftovers = $state<Leftover[]>([]);
let pinned = $state<Pinned>({ items: [], restorable: false });
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
function leaveEditor(): Promise<boolean> {
  if (!editing) return Promise.resolve(true);
  if (leaving) return leaving;
  leaving = (async () => {
    const note = await editor?.finish();
    if (!note) return false;
    contents[selected] = note;
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
async function edit(path: string) {
  if (editing) return;
  try {
    error = "";
    const note = await notesOpen(path);
    if (editing) return;
    contents[path] = note;
    opened = note;
    version = note.version;
    selected = path;
    expanded = path;
    editing = true;
  } catch (e) {
    error = errorText(e);
  }
}
/**
 * Leaving by keyboard (Esc or Ctrl+E): the card stays expanded and its header takes the focus, so the
 * next Esc reaches the list rules instead of closing the panel.
 */
async function leaveByKeyboard() {
  const path = selected;
  if (!(await leaveEditor())) return;
  await tick();
  const focused = document.activeElement;
  if (focused && focused !== document.body) return;
  list?.querySelector<HTMLElement>(`article[data-path="${CSS.escape(path)}"] .head`)?.focus();
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

// Search: the index follows the listing, one sync at a time.
const index: SearchIndex = new Map();
let indexVersion = $state(0);
let syncing: Promise<void> = Promise.resolve();
function sync(list: Note[]): Promise<void> {
  const run = syncing.then(async () => {
    await syncIndex(index, list, notesTexts);
    indexVersion++;
  });
  syncing = run.catch(() => {});
  return run;
}

const results = $derived.by(() => {
  void indexVersion;
  return search(index, query);
});
const kept = $derived.by(() => {
  void indexVersion;
  if (!expanded || results.hits.some((hit) => hit.path === expanded)) return null;
  const listed = notes.some((note) => note.path === expanded);
  if (!listed && !contents[expanded]) return null;
  const name = expanded.slice(expanded.lastIndexOf("/") + 1).replace(/\.md$/i, "");
  const entry = listed ? index.get(expanded) : undefined;
  return {
    path: expanded,
    title: entry?.title ?? name,
    tags: entry?.tags ?? [],
    modified: entry?.modified ?? 0,
    snippet: null,
    gone: !listed,
  };
});
const model = $derived(
  buildList({
    hits: results.hits,
    exactTitle: results.exactTitle,
    query,
    pinned: pinned.items,
    expanded,
    kept,
    editing,
  }),
);
const listRows = $derived(rows(model, editing ? selected : ""));
const keys = $derived(navKeys(model));
const active = $derived(keys.includes(selection) ? selection : defaultKey(model, query));

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
  pinned = { items: [], restorable: false };
  index.clear();
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

async function choose() {
  try {
    error = "";
    if (!(await leaveEditor())) return;
    await refresh(await notesChoose());
  } catch (e) {
    error = errorText(e);
  }
}

/** Creates the note for the typed text and opens it expanded. */
async function create(text: string) {
  try {
    error = "";
    if (!(await leaveEditor())) return;
    const taken = notes.map((note) => note.path);
    // Another app may have taken the name since the listing: the save then conflicts, and the next name is tried.
    for (let attempt = 0; attempt < 50; attempt++) {
      const name = noteFileName(text, taken);
      const result = await notesSave(name, "", "");
      if (result.status === "saved") {
        const list = await notesList();
        notes = list;
        await sync(list);
        query = "";
        expanded = name;
        selection = name;
        return;
      }
      taken.push(name);
    }
    error = errorText({ code: "saveFailed", detail: text });
  } catch (e) {
    error = errorText(e);
  }
}

/** Moves a card to `level`; for a pinned card, line against open is also remembered. */
async function setLevel(card: Card, level: Level) {
  const path = card.hit.path;
  try {
    error = "";
    // Closing the edited card or expanding another leaves editing first: it saves, or refuses on a
    // conflict or error and then nothing changes.
    if (editing && (path !== selected || level !== "expanded") && !(await leaveEditor())) return;
    selection = path;
    if (level === "expanded") expanded = path;
    else if (expanded === path) expanded = "";
    if (!card.pinned) return;
    const item = pinned.items.find((i) => i.target === path);
    const collapsed = level === "line";
    if (item && item.collapsed !== collapsed) pinned = await pinnedSetCollapsed(path, collapsed);
  } catch (e) {
    error = errorText(e);
  }
}

function toggle(card: Card) {
  const level = card.level === "line" ? deeper(card) : "line";
  if (level) void setLevel(card, level);
}

async function pinOrUnpin(card: Card) {
  try {
    error = "";
    pinned = card.pinned
      ? await pinnedRemove(card.hit.path)
      : await pinnedAdd(card.hit.path, card.hit.title);
  } catch (e) {
    error = errorText(e);
  }
}

async function pinnedAction(action: () => Promise<Pinned>) {
  try {
    error = "";
    pinned = await action();
  } catch (e) {
    error = errorText(e);
  }
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
    void openExternal(href).catch((e) => {
      error = errorText(e);
    });
  return true;
}

let tools = $state<HTMLElement>();
let list = $state<HTMLElement>();
const bar = () => tools?.querySelector("input");

/** Typing searches: the edited card is left the guarded way and stays expanded, reading. */
function typed() {
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
    const element = list.querySelector(`article[data-path="${CSS.escape(card.hit.path)}"]`);
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

/**
 * Esc, in layers: leave editing; clear the search and return to the bar; from the empty bar, close
 * the panel. A pinned panel stays.
 */
function escapeKey(inBar: boolean) {
  if (editing && !inBar) {
    void leaveByKeyboard();
    return;
  }
  const input = bar();
  if (query || (input && !inBar)) {
    query = "";
    selection = "";
    input?.focus();
    return;
  }
  void panelClose();
}

/** List keys, from the search bar (`inBar`) or from anywhere in the panel outside a text field. */
function listKey(event: KeyboardEvent, inBar: boolean) {
  // Without a folder there is no list: buttons keep Enter and the arrows.
  if (!list && event.key !== "Escape") return;
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
      const input = event.target;
      if (
        inBar &&
        input instanceof HTMLInputElement &&
        (input.selectionStart !== input.selectionEnd || input.selectionEnd !== input.value.length)
      )
        return;
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
        if (query.trim()) void create(query.trim());
        return;
      }
      const action = enter(model, active);
      if (action?.action === "create") void create(action.text);
      else if (action) {
        const card = cardFor(action.path);
        if (card) void setLevel(card, "expanded");
      }
      return;
    }
  }
  // Type to search: the key lands in the bar once it has the focus. Space stays with buttons.
  const printable = event.key.length === 1 && event.key !== " ";
  if (!inBar && printable && !event.ctrlKey && !event.altKey && !event.metaKey) bar()?.focus();
}

/** The search bar's keys. Runs in capture, before the field's own Esc. */
function barKey(event: KeyboardEvent) {
  if (event.target instanceof HTMLInputElement && !event.isComposing) listKey(event, true);
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
  if (event.defaultPrevented || event.isComposing) return;
  const input = bar();
  const inBar = input !== undefined && input !== null && event.target === input;
  const field = !inBar && inField(event);
  const ctrl = event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey;
  const key = event.key.toLowerCase();
  if (ctrl && key === "e") {
    const path = editing ? selected : (cardFor(active)?.hit.path ?? expanded);
    if (!path) return;
    event.preventDefault();
    if (editing) void leaveByKeyboard();
    else void edit(path);
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
  const unlisten = onNotesChanged(changed);
  // An expanded card returns to preview when the panel closes, after editing is left the guarded way.
  const hidden = onPanelHidden(() => {
    void leaveEditor().then((left) => {
      if (left) expanded = "";
    });
  });
  // The shortcut goes to the search bar with its text selected, so typing replaces it; pressed with
  // the bar already focused, it closes the panel.
  const search = onPanelSearch((toggle) => {
    const input = bar();
    if (toggle && (!input || document.activeElement === input)) {
      void panelClose();
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
    void unlisten.then((stop) => stop());
    void hidden.then((stop) => stop());
    void search.then((stop) => stop());
  };
});
</script>

<svelte:window onkeydown={shortcut} />

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

<section aria-label={t("notes.label")}>
  {#if folder?.available}
    <div class="tools" bind:this={tools} onkeydowncapture={barKey} oninput={typed} role="presentation">
      <TextField
        bind:value={query}
        aria-label={t("notes.search")}
        placeholder={t("notes.searchPlaceholder")}
      />
      <IconButton icon="folder" label={t("notes.changeFolder", { path: folder.path })} onclick={choose} />
    </div>
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
    <div class="list" bind:this={list}>
      {#if pinned.restorable}
        <div class="restore">
          <p class="message">{t("pinned.deleted")}</p>
          <Button onclick={() => pinnedAction(pinnedRestore)}>{t("pinned.restore")}</Button>
        </div>
      {/if}
      {#if model.create !== null}
        {@const text = model.create}
        <button class="create" class:selected={active === "create"} onclick={() => create(text)}>
          {t("list.create", { text })}
        </button>
      {/if}
      <!-- One block for both sections, so a card that changes section moves instead of being rebuilt. -->
      {#each listRows as row (row.key)}
        {#if "heading" in row}
          <h2>{t(row.heading === "pinned" ? "list.pinned" : "list.notes")}</h2>
        {:else if isMissing(row.entry)}
          {@const entry = row.entry}
          <div class="missing">
            <span class="title" title={entry.target}>{entry.title}</span>
            <small>{t("card.missing")}</small>
            <IconButton
              quiet
              icon="x"
              label={t("card.remove")}
              onclick={() => pinnedAction(() => pinnedRemove(entry.target))}
            />
          </div>
        {:else}
          {@const card = row.entry}
          {@const edited = editing && card.hit.path === selected}
          <CardView
            {card}
            text={contents[card.hit.path]?.text}
            selected={active === card.hit.path}
            onheader={() => toggle(card)}
            onexpand={() => setLevel(card, "expanded")}
            onpreview={() => setLevel(card, "preview")}
            onpin={() => pinOrUnpin(card)}
            onedit={() => (edited ? leaveEditor() : edit(card.hit.path))}
            onlink={linkClick}
            editing={edited}
            status={edited ? saveText : ""}
            editor={edited ? noteEditor : undefined}
          />
        {/if}
      {/each}
      {#if notes.length === 0 && !query.trim()}<p class="message">{t("notes.emptyFolder")}</p>{/if}
    </div>
  {:else}
    <div class="empty">
      <p>{folder ? t("notes.unavailable") : t("notes.chooseHint")}</p>
      {#if folder}<p class="path">{folder.path}</p>{/if}
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <Button onclick={choose}>{t("notes.choose")}</Button>
    </div>
  {/if}
</section>

<style>
  section {
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

  h2 {
    margin: 0;
    padding: var(--space-2) var(--panel-inset-right) var(--space-1) var(--panel-inset-left);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    font-weight: 600;
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

  .missing {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin: 0 var(--panel-inset-right) var(--space-2) var(--panel-inset-left);
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-2);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
  }

  .missing .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--color-muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .missing small {
    flex: none;
    color: var(--color-muted);
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
    padding: var(--space-3) var(--panel-inset-right) var(--space-2) var(--panel-inset-left);
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
