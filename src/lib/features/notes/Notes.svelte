<script lang="ts">
import { onMount, untrack } from "svelte";
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
  openExternal,
  type Pinned,
  panelSetEditing,
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

// The editor still opens the way it did before cards: it takes over the panel until the note is left.
/** The note being edited. */
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
async function toggleEditor() {
  if (editing) await leaveEditor();
}

let chrome = $state<HTMLElement>();
/** The edit button sits over the first line and scrolls away with the content, like text would. */
function follow(event: Event) {
  const top = (event.target as Element).scrollTop ?? 0;
  if (chrome) chrome.style.transform = top ? `translateY(${-top}px)` : "";
}

// A new scroller starts at the top.
$effect(() => {
  void editing;
  if (chrome) chrome.style.transform = "";
});

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
  if (!expanded || notes.some((note) => note.path === expanded) || !contents[expanded]) return null;
  const title = expanded.slice(expanded.lastIndexOf("/") + 1).replace(/\.md$/i, "");
  return { path: expanded, title, tags: [], modified: 0, snippet: null };
});
const model = $derived(
  buildList({
    hits: results.hits,
    exactTitle: results.exactTitle,
    query,
    pinned: pinned.items,
    expanded,
    kept,
  }),
);
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
  const level = card.level === "line" ? deeper(card) : shallower(card);
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

/** Arrows, Enter and Esc in the search bar; focus stays there. Runs before the field's own Esc. */
function barKey(event: KeyboardEvent) {
  const input = event.target;
  if (!(input instanceof HTMLInputElement) || event.isComposing) return;
  switch (event.key) {
    case "Escape":
      // Clears the search and stays in the field; the next Esc leaves it.
      if (!query) return;
      event.preventDefault();
      event.stopPropagation();
      query = "";
      selection = "";
      // This press is handled: the panel must not also close.
      void panelSetEditing(true);
      return;
    case "ArrowDown":
    case "ArrowUp":
      event.preventDefault();
      selection = move(keys, active, event.key === "ArrowDown" ? 1 : -1);
      return;
    case "ArrowRight":
    case "ArrowLeft": {
      // Otherwise the arrows move the caret; they open and close only from its natural place, the end.
      if (event.ctrlKey || event.altKey || event.shiftKey || event.metaKey) return;
      if (input.selectionStart !== input.selectionEnd || input.selectionEnd !== input.value.length)
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
    }
  }
}

function shortcut(event: KeyboardEvent) {
  if (event.defaultPrevented) return;
  if (event.key === "Escape" && !editing) {
    // Outside a field, Esc clears the search and returns to the bar.
    const field = document.activeElement;
    if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) return;
    if (!query && !list?.contains(field)) return;
    event.preventDefault();
    query = "";
    selection = "";
    bar()?.focus();
    return;
  }
  if (!event.ctrlKey || event.altKey || event.shiftKey || event.key.toLowerCase() !== "e") return;
  const path = editing ? selected : (cardFor(active)?.hit.path ?? expanded);
  if (!path) return;
  event.preventDefault();
  if (editing) void toggleEditor();
  else void edit(path);
}

onMount(() => {
  const unlisten = onNotesChanged(changed);
  // An expanded card returns to preview when the panel closes; editing keeps the panel open.
  const hidden = onPanelHidden(() => {
    if (!editing) expanded = "";
  });
  notesFolder()
    .then(refresh)
    .catch((e) => {
      error = errorText(e);
    });
  return () => {
    void unlisten.then((stop) => stop());
    void hidden.then((stop) => stop());
  };
});
</script>

<svelte:window onkeydown={shortcut} />

<section aria-label={t("notes.label")}>
  {#if folder?.available}
    {#if !editing}
      <div class="tools" bind:this={tools} onkeydowncapture={barKey} oninput={() => { selection = ""; }} role="presentation">
        <TextField
          bind:value={query}
          aria-label={t("notes.search")}
          placeholder={t("notes.searchPlaceholder")}
          onfocus={() => panelSetEditing(true)}
          onblur={() => panelSetEditing(false)}
        />
        <IconButton icon="folder" label={t("notes.changeFolder", { path: folder.path })} onclick={choose} />
      </div>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if leftovers.length > 0 && !editing}
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
    <div class="list" bind:this={list} hidden={editing}>
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
      {#if model.pinned.length > 0}
        <h2>{t("list.pinned")}</h2>
        {#each model.pinned as entry (isMissing(entry) ? `missing:${entry.target}` : entry.hit.path)}
          {#if isMissing(entry)}
            <div class="missing">
              <span class="title" title={entry.target}>{entry.title}</span>
              <small>{t("card.missing")}</small>
              <IconButton
                icon="x"
                label={t("card.remove")}
                onclick={() => pinnedAction(() => pinnedRemove(entry.target))}
              />
            </div>
          {:else}
            <CardView
              card={entry}
              text={contents[entry.hit.path]?.text}
              selected={active === entry.hit.path}
              onheader={() => toggle(entry)}
              onexpand={() => setLevel(entry, "expanded")}
              onclose={() => toggle(entry)}
              onpin={() => pinOrUnpin(entry)}
              onedit={() => edit(entry.hit.path)}
              onlink={linkClick}
            />
          {/if}
        {/each}
      {/if}
      {#if model.notes.length > 0}
        <h2>{t("list.notes")}</h2>
        {#each model.notes as card (card.hit.path)}
          <CardView
            {card}
            text={contents[card.hit.path]?.text}
            selected={active === card.hit.path}
            onheader={() => toggle(card)}
            onexpand={() => setLevel(card, "expanded")}
            onclose={() => toggle(card)}
            onpin={() => pinOrUnpin(card)}
            onedit={() => edit(card.hit.path)}
            onlink={linkClick}
          />
        {/each}
      {/if}
      {#if notes.length === 0 && !query.trim()}<p class="message">{t("notes.emptyFolder")}</p>{/if}
    </div>
    {#if editing && opened}
      <div class="note-area" onscrollcapture={follow}>
        <div class="edit-tools" bind:this={chrome}>
          {#if saveStatus === "saved" || saveStatus === "saving" || saveStatus === "pending"}
            <small role="status">{t(saveStatus === "saved" ? "editor.saved" : "editor.saving")}</small>
          {/if}
          <IconButton icon="pencil" label={t("editor.toggle")} pressed={true} onclick={toggleEditor} />
        </div>
        <Editor
          bind:this={editor}
          path={selected}
          note={opened}
          onstatus={(state) => { saveStatus = state; }}
          onleave={() => void leaveEditor()}
          ondiscard={() => {
            void load(selected);
            selected = "";
            editing = false;
          }}
        />
      </div>
    {/if}
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

  .list[hidden] {
    display: none;
  }

  h2 {
    margin: 0;
    padding: var(--space-2) var(--space-4) var(--space-1);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    font-weight: 600;
  }

  .create {
    display: block;
    box-sizing: border-box;
    width: calc(100% - 2 * var(--space-3));
    min-height: var(--size-control);
    margin: 0 var(--space-3) var(--space-2);
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
    margin: 0 var(--space-3) var(--space-2);
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
    padding: var(--space-1) var(--space-3);
  }

  .note-area { position: relative; display: flex; flex: 1; flex-direction: column; min-height: 0; overflow: hidden; border: 1px solid var(--color-accent); border-radius: var(--radius-sm); }
  .edit-tools { position: absolute; z-index: 1; top: var(--space-2); right: var(--space-3); display: flex; gap: var(--space-2); align-items: center; border-radius: var(--radius-sm); background: var(--color-bg); }
  .edit-tools small { color: var(--color-muted); font-size: var(--font-size-sm); }

  .tools {
    display: flex;
    flex: none;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-3) var(--space-2);
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
    padding: 0 var(--space-3) 0 var(--space-4);
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
    padding: var(--space-1) var(--space-4);
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
    padding: var(--space-4);
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
