<script lang="ts">
import type { Snippet } from "svelte";
import { date, t } from "../../i18n";
import IconButton from "../../ui/IconButton.svelte";
import TextField from "../../ui/TextField.svelte";
import { folderOf } from "./grouping";
import Highlight from "./Highlight.svelte";
import { type Card, fileLabel, pageHost } from "./list";
import { renderNote } from "./render";
import "./content.css";

let {
  card,
  drag,
  text,
  selected,
  onheader,
  onrename,
  ontitledone,
  onexpand,
  onpreview,
  onpin,
  onedit,
  onlink,
  onpage,
  editing = false,
  status = "",
  editor,
  folder = false,
}: {
  card: Card;
  drag?: (element: HTMLElement) => () => void;
  /** Raw text of the note once read; the line state does not need it. */
  text: string | undefined;
  /** Chosen with the arrows or the mouse. */
  selected: boolean;
  /** Header click: open a line, close anything else. */
  onheader: () => void;
  /** The title field, while editing, was left with this title. */
  onrename: (title: string) => Promise<void>;
  /** Enter, ↓ or Esc in the title field: go on to the text. */
  ontitledone: () => void;
  onexpand: () => void;
  onpreview: () => void;
  onpin: () => void;
  onedit: () => void;
  /** Handles a click on a link in the content; true if it was one. */
  onlink: (event: MouseEvent) => boolean;
  onpage: (url: string) => void;
  /** This card holds the one editor; the pencil stays accented. */
  editing?: boolean;
  /** Saving… or Saved while editing; empty when a notice explains a problem instead. */
  status?: string;
  /** The editor, drawn in place of the content while editing. */
  editor?: Snippet;
  /** Shows the note's folder before its title, unless a folder group already names it. */
  folder?: boolean;
} = $props();

const open = $derived(card.level !== "line");
const row = $derived(!card.pinned && card.level === "line");
let root = $state<HTMLElement>();
let body = $state<HTMLElement>();
let clipped = $state(false);

// A card that comes into the selection by keyboard scrolls into view.
$effect(() => {
  if (selected) root?.scrollIntoView({ block: "nearest" });
});

// The fade at the bottom of a preview only makes sense when something is cut off.
$effect(() => {
  void text;
  const element = body;
  if (!element || card.level !== "preview") return;
  const measure = () => {
    clipped = element.scrollHeight > element.clientHeight + 1;
  };
  const frame = requestAnimationFrame(measure);
  const observer = new ResizeObserver(measure);
  observer.observe(element);
  return () => {
    cancelAnimationFrame(frame);
    observer.disconnect();
  };
});

function bodyClick(event: MouseEvent) {
  if (onlink(event)) return;
  if (card.level === "preview") onexpand();
}
/** A double click on the text edits it; its first click already expanded a preview. */
function bodyDoubleClick(event: MouseEvent) {
  if (!editing && !card.url && !(event.target as Element).closest("a")) onedit();
}

const tags = $derived(card.hit.tags);
const file = $derived(card.url ? null : fileLabel(card.hit.path, card.hit.title));
// A pinned link's path is its address, not a file; saved pages are notes and keep their folder.
const prefix = $derived(folder && /\.md$/i.test(card.hit.path) ? folderOf(card.hit.path) : "");

// While editing, the title is a field; leaving it renames. It follows the note while not focused.
let title = $state("");
let typing = false;
$effect(() => {
  const next = card.hit.title;
  if (!typing) title = next;
});
function fieldKey(event: KeyboardEvent) {
  if (event.isComposing) return;
  if (event.key === "Escape") title = card.hit.title;
  if (event.key === "Enter" || event.key === "ArrowDown" || event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    ontitledone();
  }
}
const toggleLabel = $derived(open ? t("card.collapse") : t("card.show"));
</script>

<div
  {@attach drag}
  class="card"
  role="group"
  aria-label={card.hit.title}
  aria-current={selected || undefined}
  bind:this={root}
  class:current={selected}
  class:selected={selected && !editing}
  class:line={!open}
  class:row
  class:page={!!card.hit.url}
  class:expanded={card.level === "expanded"}
  class:editing
  data-path={card.hit.path}
>
  <header>
    {#if !card.url}<div class="gutter">
      <IconButton tabindex={-1} quiet icon={open ? "chevronDown" : "chevronRight"} label={toggleLabel} onclick={onheader} />
    </div>{/if}
    {#if editing}
      <span class="rename">
        <TextField
          bind:value={title}
          aria-label={t("card.rename")}
          spellcheck={false}
          onkeydown={fieldKey}
          onfocus={() => { typing = true; }}
          onblur={async () => { typing = false; await onrename(title); if (!typing) title = card.hit.title; }}
        />
      </span>
    {/if}
    <!-- Hidden rather than removed: pinned drag is attached to this button. -->
    <button tabindex="-1" class="head" hidden={editing} onclick={onheader}>
      <span class="title">{#if prefix}<small class="folder">{prefix}/</small>{/if}{card.hit.title}{#if file}<small class="file">{file}</small>{/if}</span>
      <span class="meta">
      {#if card.url}<small class="tag" title={card.url}>{pageHost(card.url)}</small>{/if}
      {#if tags.length > 0}
        <small class="tag">{tags[0]}{#if tags.length > 1}<span class="more"> +{tags.length - 1}</span>{/if}</small>
      {/if}
      {#if card.hit.modified}<small class="date">{date(card.hit.modified)}</small>{/if}
      </span>
    </button>
    {#if status}<small class="status" role="status">{status}</small>{/if}
    <div class="actions">
      {#if card.hit.url}
        <IconButton tabindex={-1} quiet icon="globe" label={t("web.openPage")} onclick={() => onpage(card.hit.url!)} />
      {/if}
      {#if card.level === "expanded"}
        <div class="pencil" class:active={editing}>
          <IconButton tabindex={-1} quiet accent={editing} icon="pencil" label={t("editor.toggle")} pressed={editing} onclick={onedit} />
        </div>
      {/if}
      <IconButton tabindex={-1} quiet icon="bookmark" label={t(card.pinned ? "card.unpin" : "card.pin")} pressed={card.pinned} onclick={onpin} />
    </div>
  </header>
  {#if card.gone}<p class="notice" role="status">{t("notes.gone")}</p>{/if}
  {#if !open && card.hit.snippet}
    <p class="match"><Highlight snippet={card.hit.snippet} /></p>
  {/if}
  {#if editor}
    {@render editor()}
  {:else if open}
    <!-- Built from the editor's parser as DOM nodes; note text never becomes HTML. -->
    <div class="reading">
      <div
        bind:this={body}
        class="body note-content"
        class:preview={card.level === "preview"}
        class:clipped={card.level === "preview" && clipped}
        onclick={bodyClick}
        ondblclick={bodyDoubleClick}
        role="presentation"
        {@attach (element) => {
          let current = true;
          void renderNote(text ?? "", card.hit.path).then((fragment) => {
            if (!current) return;
            element.replaceChildren(fragment);
            for (const link of element.querySelectorAll("a")) link.tabIndex = -1;
          });
          return () => { current = false; };
        }}
      ></div>
      {#if card.level === "preview" && clipped}
        <div class="reveal"><button tabindex="-1" onclick={onexpand}>{t("card.showAll")}</button></div>
      {:else if card.level === "expanded" && card.pinned}
        <div class="reveal"><button tabindex="-1" onclick={onpreview}>{t("card.showLess")}</button></div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .card {
    margin: 0 var(--panel-inset-right) var(--space-2) var(--panel-inset-left);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
  }

  /* A row is the same card, reduced to its line. */
  .card.row {
    margin-bottom: 0;
    border-color: transparent;
    background: transparent;
  }

  .card.selected {
    border-color: var(--color-accent);
  }

  .card.selected.line {
    background: var(--color-selected);
  }

  .card.editing {
    border-color: var(--color-accent);
  }

  .card.selected.line .title {
    color: var(--color-accent);
  }

  header {
    position: relative;
    display: flex;
    gap: var(--space-1);
    align-items: center;
    padding: var(--space-1);
    border-radius: var(--radius-sm);
  }

  .gutter {
    position: absolute;
    right: calc(100% + 1px);
    top: var(--space-1);
    width: var(--panel-inset-left);
    display: flex;
    justify-content: center;
  }

  .gutter, .actions :global(button) {
    opacity: 0;
  }
  .card:hover .gutter,
  .card.current .gutter,
  .gutter:has(:focus-visible),
  .card:is(:hover, .current, .selected, :has(:focus-visible), .editing) .actions :global(button) {
    opacity: 1;
  }

  .actions {
    display: flex;
    flex: none;
    gap: var(--space-1);
    align-items: center;
  }

  .card.expanded > header {
    position: sticky;
    z-index: 1;
    top: 0;
    border-bottom: 1px solid var(--color-line);
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    background: var(--color-surface);
  }

  .head {
    display: flex;
    flex: 1;
    gap: var(--space-2);
    align-items: baseline;
    box-sizing: border-box;
    min-width: 0;
    min-height: var(--size-control);
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-fg);
    font: inherit;
    font-size: var(--font-size-sm);
    text-align: left;
  }

  .card.row:not(.selected):hover {
    background: var(--color-surface);
  }

  .rename {
    display: flex;
    flex: 1;
    min-width: 0;
  }

  /* The card already has the accent border: the focused field shows only through the accent line
     under the header. */
  .card.editing > header:has(.rename :global(input:focus)) {
    border-bottom-color: var(--color-accent);
  }

  .rename {
    margin-right: var(--header-actions);
  }

  .rename :global(input) {
    border: 0;
    outline: none;
    background: transparent;
    font-weight: 600;
  }

  .head[hidden] {
    display: none;
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    align-items: baseline;
    justify-content: flex-end;
    gap: var(--space-2);
    min-width: 0;
    max-width: 50%;
  }

  /* Stable title clearance lets metadata move without animating layout. */
  .meta .tag { flex-shrink: 1; }
  .card {
    --page-actions: 0;
    --edit-actions: 0;
    --header-actions: calc((var(--size-control) + var(--space-1)) * (1 + var(--page-actions) + var(--edit-actions)));
  }
  .card.page { --page-actions: 1; }
  .card.expanded { --edit-actions: 1; }

  .title { margin-right: var(--header-actions); }
  .meta { transition: transform var(--duration) ease; }
  /* Rows share section edges at rest; the existing box keeps its inset when active. */
  .card.row .title { transform: translateX(calc(-1 * (var(--space-3) + 1px))); transition: transform var(--duration) ease; }
  .card.row .meta { transform: translateX(calc(var(--space-1) + 1px)); }
  .card.row:is(:hover, .current, .selected, :has(:focus-visible)) .title { transform: none; }
  .card.row:is(:hover, .current, .selected, :has(:focus-visible)) { background: var(--color-surface); }
  .card.row.selected { background: var(--color-selected); }
  .actions { position: absolute; right: var(--space-1); }
  .actions :global(button) { transition: opacity var(--duration), color var(--duration); }
  .card:is(:hover, .current, .selected, :has(:focus-visible), .editing) .meta { transform: translateX(calc(-1 * var(--header-actions))); }
  .status { margin-right: var(--header-actions); }
  @media (prefers-reduced-motion: reduce) {
    .meta, .card.row .title, .actions :global(button) { transition: none; }
  }

  .file {
    margin-left: var(--space-2);
  }

  .folder,
  .file {
    font-weight: normal;
  }

  .date,
  .folder,
  .file,
  .tag,
  .status {
    flex: none;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }

  .tag {
    min-width: 0;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .match,
  .notice {
    margin: 0;
    padding: 0 var(--space-2) var(--space-1);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }

  .match {
    display: flex;
  }

  .reading {
    position: relative;
  }
  /* Below the fade, never over it: faded text beside the button reads as noise. */
  .reveal {
    display: flex;
    justify-content: center;
    padding-bottom: var(--space-2);
  }
  .reveal button {
    border: 0;
    border-radius: var(--radius-sm);
    padding: var(--space-1) var(--space-2);
    background: var(--color-surface);
    color: var(--color-muted);
    font: inherit;
    font-size: var(--font-size-sm);
  }
  .reveal button:hover {
    background: var(--color-bg);
    color: var(--color-fg);
  }

  .body {
    padding: var(--space-1) var(--space-3) var(--space-3);
    overflow-wrap: anywhere;
  }

  .body.preview {
    max-height: calc(8 * var(--line-height) * var(--font-size-md));
    overflow: hidden;
    cursor: pointer;
  }

  .body.clipped {
    mask-image: linear-gradient(to bottom, black calc(100% - 2.5em), transparent);
    padding-bottom: 0;
  }

  /* The header already shows the title. */
  .body :global(.note-header h1) {
    display: none;
  }

  /* The line already shows the first tag; the expanded card lists them all. */
  .body.preview :global(.note-header) {
    display: none;
  }

  /* The card header sits right above: no leading gap. */
  .body > :global(:first-child),
  .body.preview > :global(.note-header + *) {
    margin-top: 0;
  }

  .body :global(.note-header) {
    padding-bottom: var(--space-2);
  }

</style>
