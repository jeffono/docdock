<script lang="ts">
import { onMount } from "svelte";
import { dateTime, errorText, t } from "../../i18n";
import {
  type Leftover,
  type Note,
  type NotesFolder,
  notesChoose,
  notesFolder,
  notesLeftovers,
  notesList,
  notesOpen,
  notesRestore,
  onNotesChanged,
  openExternal,
  panelSetEditing,
} from "../../ipc";
import Button from "../../ui/Button.svelte";
import IconButton from "../../ui/IconButton.svelte";
import TextField from "../../ui/TextField.svelte";

let folder = $state<NotesFolder | null>(null);
let notes = $state<Note[]>([]);
let content = $state("");
let selected = $state("");
let error = $state("");
let query = $state("");
let gone = $state(false);
let leftovers = $state<Leftover[]>([]);
/** Version of the open note, to tell a real change from a reread of the same bytes. */
let version = "";

const visible = $derived(
  notes.filter((note) => note.path.toLowerCase().includes(query.trim().toLowerCase())),
);

/**
 * Bumped by every action that replaces what is shown: choosing a folder or opening a note. A read
 * that started before the bump answers for an old folder or note and is dropped.
 */
let generation = 0;

async function refresh(value: NotesFolder | null) {
  const current = ++generation;
  folder = value;
  notes = [];
  selected = "";
  content = "";
  leftovers = [];
  if (!value?.available) return;
  const list = await notesList();
  if (current !== generation) return;
  notes = list;
  const found = await notesLeftovers();
  if (current === generation) leftovers = found;
}

async function restore(path: string) {
  try {
    error = "";
    const note = await notesRestore(path);
    leftovers = await notesLeftovers();
    notes = await notesList();
    await open(note);
  } catch (e) {
    error = errorText(e);
  }
}

async function choose() {
  try {
    error = "";
    await refresh(await notesChoose());
  } catch (e) {
    error = errorText(e);
  }
}

async function open(path: string) {
  const current = ++generation;
  try {
    error = "";
    const note = await notesOpen(path);
    if (current !== generation) return;
    content = note.html;
    version = note.version;
    selected = path;
    gone = false;
  } catch (e) {
    error = errorText(e);
  }
}

/**
 * Rereads after a change on disk. The open note is replaced only if its bytes changed, so a save of
 * the same text does not disturb it; a note that disappeared keeps its last content on screen.
 */
async function reload() {
  const current = generation;
  // A newer action made this read stale; reading again picks up the change for what is shown now.
  const stale = () => {
    if (current !== generation) again = true;
    return current !== generation;
  };
  const value = await notesFolder();
  if (stale()) return;
  if (!value?.available) return refresh(value);
  const list = await notesList();
  if (stale()) return;
  notes = list;
  const path = selected;
  if (!path) return;
  if (!notes.some((note) => note.path === path)) {
    gone = true;
    return;
  }
  const note = await notesOpen(path);
  if (stale()) return;
  gone = false;
  if (note.version !== version) {
    content = note.html;
    version = note.version;
  }
}

let reloading: Promise<void> | null = null;
let again = false;

/** One reload at a time; changes during it cause one more. */
function changed() {
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
        error = errorText(e);
      }
    } while (again);
    reloading = null;
  })();
}

function linkClick(event: MouseEvent) {
  const link = (event.target as Element).closest("a");
  if (!link) return;
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
}

function directory(path: string) {
  return path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
}

onMount(() => {
  const unlisten = onNotesChanged(changed);
  notesFolder()
    .then(refresh)
    .catch((e) => {
      error = errorText(e);
    });
  return () => void unlisten.then((stop) => stop());
});
</script>

<section aria-label={t("notes.label")}>
  {#if folder?.available}
    <div class="tools">
      <TextField
        bind:value={query}
        aria-label={t("notes.search")}
        placeholder={t("notes.searchPlaceholder")}
        onfocus={() => panelSetEditing(true)}
        onblur={() => panelSetEditing(false)}
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
    {#if visible.length > 0}
      <ul class:compact={selected}>
        {#each visible as note (note.path)}
          <li>
            <button
              class:current={note.path === selected}
              aria-current={note.path === selected}
              title={note.path}
              onclick={() => open(note.path)}
            >
              <span>{note.name}</span><small>{directory(note.path)}</small>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="message">{notes.length > 0 ? t("notes.noMatch") : t("notes.emptyFolder")}</p>
    {/if}
    {#if selected}
      {#if gone}<p class="message" role="status">{t("notes.gone")}</p>{/if}
      <div class="note" onclick={linkClick} role="presentation">
        <!-- The HTML has already been sanitized in Rust. -->
        {@html content}
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

  .tools {
    display: flex;
    flex: none;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-3) var(--space-2);
  }

  ul {
    flex: 1;
    margin: 0;
    padding: 0 var(--space-3) var(--space-2);
    overflow-y: auto;
    list-style: none;
  }

  ul.compact {
    flex: 0 1 auto;
    max-height: 30%;
    border-bottom: 1px solid var(--color-line);
  }

  li button {
    display: flex;
    gap: var(--space-2);
    justify-content: space-between;
    box-sizing: border-box;
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-muted);
    font: inherit;
    font-size: var(--font-size-sm);
    text-align: left;
  }

  li button:hover {
    background: var(--color-surface);
    color: var(--color-fg);
  }

  li button.current {
    background: var(--color-selected);
    color: var(--color-accent);
    font-weight: 600;
  }

  li span,
  li small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  li small {
    flex: none;
    max-width: 45%;
    font-size: var(--font-size-sm);
    font-weight: 400;
    opacity: 0.75;
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

  .note {
    flex: 1;
    min-height: 0;
    padding: var(--space-3) var(--space-4) var(--space-4);
    overflow-y: auto;
    overflow-wrap: anywhere;
  }

  .note :global(h1),
  .note :global(h2),
  .note :global(h3),
  .note :global(h4),
  .note :global(h5),
  .note :global(h6) {
    margin: var(--space-4) 0 var(--space-2);
    font-weight: 600;
    line-height: var(--line-height-tight);
    letter-spacing: var(--tracking-tight);
  }

  .note :global(h1) {
    margin-top: 0;
    font-size: var(--font-size-xl);
  }

  .note :global(h2) {
    font-size: var(--font-size-lg);
  }

  .note :global(h3),
  .note :global(h4),
  .note :global(h5),
  .note :global(h6) {
    font-size: var(--font-size-md);
  }

  .note :global(p),
  .note :global(ul),
  .note :global(ol),
  .note :global(pre),
  .note :global(table),
  .note :global(blockquote) {
    margin: 0 0 var(--space-2);
  }

  .note :global(ul),
  .note :global(ol) {
    padding-left: var(--space-4);
  }

  .note :global(li:has(> input)) {
    list-style: none;
  }

  .note :global(a) {
    color: var(--color-accent);
  }

  .note :global(code) {
    padding: 1px var(--space-1);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    font-family: var(--font-mono);
    font-size: 0.9em;
  }

  .note :global(pre) {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    overflow-x: auto;
    overflow-wrap: normal;
  }

  .note :global(pre code) {
    padding: 0;
    background: transparent;
  }

  .note :global(blockquote) {
    padding-left: var(--space-3);
    border-left: 2px solid var(--color-line);
    color: var(--color-muted);
  }

  .note :global(hr) {
    margin: var(--space-3) 0;
    border: 0;
    border-top: 1px solid var(--color-line);
  }

  .note :global(img) {
    max-width: 100%;
    border-radius: var(--radius-sm);
  }

  .note :global(table) {
    display: block;
    border-collapse: collapse;
    overflow-x: auto;
    overflow-wrap: normal;
    font-size: var(--font-size-sm);
  }

  .note :global(th),
  .note :global(td) {
    padding: var(--space-1) var(--space-2) var(--space-1) 0;
    border-bottom: 1px solid var(--color-line);
    text-align: left;
  }

  .note :global(th) {
    color: var(--color-muted);
    font-weight: 600;
  }
</style>
