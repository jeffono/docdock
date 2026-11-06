<script lang="ts">
import type { Snippet } from "svelte";
import { date, t } from "../../i18n";
import IconButton from "../../ui/IconButton.svelte";
import Highlight from "./Highlight.svelte";
import type { Card } from "./list";
import { renderNote } from "./render";
import "./content.css";

let {
  card,
  text,
  selected,
  onheader,
  onexpand,
  onpreview,
  onpin,
  onedit,
  onlink,
  editing = false,
  status = "",
  editor,
}: {
  card: Card;
  /** Raw text of the note once read; the line state does not need it. */
  text: string | undefined;
  /** Chosen with the arrows or the mouse. */
  selected: boolean;
  /** Header click: open a line, close anything else. */
  onheader: () => void;
  onexpand: () => void;
  onpreview: () => void;
  onpin: () => void;
  onedit: () => void;
  /** Handles a click on a link in the content; true if it was one. */
  onlink: (event: MouseEvent) => boolean;
  /** This card holds the one editor; the pencil stays accented. */
  editing?: boolean;
  /** Saving… or Saved while editing; empty when a notice explains a problem instead. */
  status?: string;
  /** The editor, drawn in place of the content while editing. */
  editor?: Snippet;
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

const tags = $derived(card.hit.tags);
const toggleLabel = $derived(open ? t("card.collapse") : t("card.show"));
</script>

<div
  class="card"
  role="group"
  aria-label={card.hit.title}
  aria-current={selected || undefined}
  bind:this={root}
  class:current={selected}
  class:selected={selected && !editing}
  class:line={!open}
  class:row
  class:expanded={card.level === "expanded"}
  class:editing
  data-path={card.hit.path}
>
  <header>
    <div class="gutter">
      <IconButton tabindex={-1} quiet icon={open ? "chevronDown" : "chevronRight"} label={toggleLabel} onclick={onheader} />
    </div>
    <button tabindex="-1" class="head" onclick={onheader}>
      <span class="title">{card.hit.title}</span>
      {#if card.hit.modified}<small class="date">{date(card.hit.modified)}</small>{/if}
      {#if tags.length > 0}
        <small class="tag">{tags[0]}{#if tags.length > 1}<span class="more"> +{tags.length - 1}</span>{/if}</small>
      {/if}
    </button>
    {#if status}<small class="status" role="status">{status}</small>{/if}
    <div class="actions">
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
  .gutter:focus-within,
  .card:hover .actions :global(button),
  .card.current .actions :global(button),
  .actions :global(button:focus-visible),
  .pencil.active :global(button) {
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

  .card.row:not(.selected) .head:hover {
    background: var(--color-surface);
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .date,
  .tag,
  .status {
    flex: none;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }

  .tag {
    max-width: 30%;
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
