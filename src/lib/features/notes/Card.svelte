<script lang="ts">
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
  onclose,
  onpin,
  onedit,
  onlink,
}: {
  card: Card;
  /** Raw text of the note once read; the line state does not need it. */
  text: string | undefined;
  /** Chosen with the arrows or the mouse. */
  selected: boolean;
  /** Header click: open a line, close anything else. */
  onheader: () => void;
  onexpand: () => void;
  onclose: () => void;
  onpin: () => void;
  onedit: () => void;
  /** Handles a click on a link in the content; true if it was one. */
  onlink: (event: MouseEvent) => boolean;
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
  if (body && card.level === "preview") clipped = body.scrollHeight > body.clientHeight + 1;
});

function bodyClick(event: MouseEvent) {
  if (onlink(event)) return;
  if (card.level === "preview") onexpand();
}

const tags = $derived(card.hit.tags);
const toggle = $derived(
  card.level === "line"
    ? { icon: "chevronDown" as const, label: t("card.show") }
    : {
        icon: "chevronUp" as const,
        label: t(card.level === "preview" ? "card.collapse" : "card.close"),
      },
);
</script>

<article bind:this={root} class:selected class:row class:expanded={card.level === "expanded"} aria-current={selected}>
  <header>
    <button class="head" onclick={onheader}>
      <span class="title">{card.hit.title}</span>
      {#if card.hit.modified}<small class="date">{date(card.hit.modified)}</small>{/if}
      {#if tags.length > 0}
        <small class="tag">{tags[0]}{#if tags.length > 1}<span class="more"> +{tags.length - 1}</span>{/if}</small>
      {/if}
    </button>
    {#if card.pinned || card.level === "expanded"}
      <div class="actions">
        {#if card.level === "expanded"}
          <IconButton quiet icon="pencil" label={t("editor.toggle")} onclick={onedit} />
        {/if}
        <IconButton quiet icon="bookmark" label={t("card.pin")} pressed={card.pinned} onclick={onpin} />
        <IconButton quiet icon={toggle.icon} label={toggle.label} onclick={card.level === "line" ? onheader : onclose} />
      </div>
    {/if}
  </header>
  {#if card.gone}<p class="notice" role="status">{t("notes.gone")}</p>{/if}
  {#if !open && card.hit.snippet}
    <p class="match"><Highlight snippet={card.hit.snippet} /></p>
  {/if}
  {#if open}
    <!-- Built from the editor's parser as DOM nodes; note text never becomes HTML. -->
    <div
      bind:this={body}
      class="body note-content"
      class:preview={card.level === "preview"}
      class:clipped
      onclick={bodyClick}
      role="presentation"
      {@attach (element) => {
        element.replaceChildren(renderNote(text ?? "", card.hit.path));
      }}
    ></div>
  {/if}
</article>

<style>
  article {
    margin: 0 var(--space-3) var(--space-2);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
  }

  /* A row is the same card, reduced to its line. */
  article.row {
    margin-bottom: 0;
    border-color: transparent;
    background: transparent;
  }

  article.selected {
    border-color: var(--color-accent);
    background: var(--color-selected);
  }

  article.selected .title {
    color: var(--color-accent);
  }

  header {
    display: flex;
    gap: var(--space-1);
    align-items: center;
    padding: var(--space-1);
    border-radius: var(--radius-sm);
  }

  .actions {
    display: flex;
    flex: none;
    gap: var(--space-1);
    align-items: center;
  }

  article.expanded > header {
    position: sticky;
    z-index: 1;
    top: 0;
    border-bottom: 1px solid var(--color-line);
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    background: var(--color-surface);
  }

  article.expanded.selected > header {
    background: var(--color-selected);
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

  article.row:not(.selected) .head:hover {
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
  .tag {
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
  }

  /* The header already shows the title. */
  .body :global(.note-header h1) {
    display: none;
  }

  /* The line already shows the first tag; the expanded card lists them all. */
  .body.preview :global(.note-header) {
    display: none;
  }

  .body :global(.note-header) {
    padding-bottom: var(--space-2);
  }
</style>
