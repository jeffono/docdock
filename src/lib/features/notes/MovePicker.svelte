<script lang="ts">
import { flushSync } from "svelte";
import { t } from "../../i18n";
import Icon from "../../ui/Icon.svelte";
import { placeNear } from "../../ui/place";
import TextField from "../../ui/TextField.svelte";

/** A folder to move to ("" is the root), and whether it has to be created first. */
type MoveChoice = { folder: string; create: boolean };

/**
 * Picks the folder a note moves to, inside the panel over the scrim, as the confirmation dialog does.
 * The field filters the folders, the root first, and offers to create the typed one when no folder has
 * that name. ↑ / ↓ choose, Enter takes the choice, Esc or a click outside cancels; focus returns where
 * it was. The note's own folder is checked, and taking it answers nothing.
 */
const id = $props.id();
let dialog: HTMLDialogElement;
let field = $state<HTMLInputElement>();
let listbox = $state<HTMLElement>();
let title = $state("");
let folders = $state<string[]>([]);
let current = $state("");
let query = $state("");
let active = $state(0);
let answer: ((choice: MoveChoice | null) => void) | null = null;
let previous: Element | null = null;

/** The typed folder as a path: `\` as `/`, and spaces and empty names around separators dropped. */
const typed = $derived(
  query
    .replaceAll("\\", "/")
    .split("/")
    .map((name) => name.trim())
    .filter(Boolean)
    .join("/"),
);
const label = (folder: string) => folder || t("move.root");
const options = $derived.by((): MoveChoice[] => {
  const needle = typed.toLowerCase();
  const shown = folders.filter((folder) => label(folder).toLowerCase().includes(needle));
  const exists = folders.some((folder) => folder.toLowerCase() === needle);
  return [
    ...shown.map((folder) => ({ folder, create: false })),
    ...(typed && !exists ? [{ folder: typed, create: true }] : []),
  ];
});

export function isOpen() {
  return dialog?.open ?? false;
}

/**
 * `all`: every folder but the root; `from`: the note's folder. `near`: the card header, as for the
 * confirmation dialog.
 */
export function ask(
  noteTitle: string,
  all: string[],
  from: string,
  near?: DOMRect,
): Promise<MoveChoice | null> {
  answer?.(null);
  title = noteTitle;
  folders = ["", ...all];
  current = from;
  query = "";
  active = 0;
  previous = document.activeElement;
  flushSync();
  dialog.showModal();
  placeNear(dialog, near);
  field?.focus();
  return new Promise((resolve) => {
    answer = resolve;
  });
}

function done(choice: MoveChoice | null) {
  if (!dialog.open) return;
  dialog.close();
  if (previous instanceof HTMLElement && previous.isConnected)
    previous.focus({ preventScroll: true });
  previous = null;
  // The note's own folder: nothing to do.
  const same = choice && !choice.create && choice.folder.toLowerCase() === current.toLowerCase();
  answer?.(same ? null : choice);
  answer = null;
}

function key(event: KeyboardEvent) {
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const count = options.length;
    if (count) active = (active + (event.key === "ArrowDown" ? 1 : count - 1)) % count;
    listbox
      ?.querySelector(`#${CSS.escape(`${id}-${active}`)}`)
      ?.scrollIntoView({ block: "nearest" });
  } else if (event.key === "Enter") {
    event.preventDefault();
    const choice = options[active];
    if (choice) done(choice);
  }
}
</script>

<!-- A click on the scrim, outside the dialog's box, cancels; the dialog itself is the event target there. -->
<dialog bind:this={dialog} aria-labelledby={`${id}-title`} oncancel={(event) => { event.preventDefault(); done(null); }} onclick={(event) => { if (event.target === dialog) done(null); }}>
  <h2 id={`${id}-title`}>{t("move.title", { title })}</h2>
  <TextField bind:ref={field} bind:value={query} role="combobox" aria-label={t("move.field")} aria-expanded="true" aria-controls={`${id}-list`} aria-activedescendant={options.length ? `${id}-${active}` : undefined} aria-autocomplete="list" placeholder={t("move.placeholder")} spellcheck="false" autocomplete="off" oninput={() => { active = 0; }} onkeydown={key} />
  <ul bind:this={listbox} id={`${id}-list`} role="listbox" aria-label={t("move.folders")}>
    {#each options as option, index (`${option.create}:${option.folder}`)}
      <!-- Keyboard use goes through the field, which keeps the focus. -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <li id={`${id}-${index}`} role="option" aria-selected={index === active} class:active={index === active} onpointerdown={(event) => event.preventDefault()} onpointermove={() => { active = index; }} onclick={() => done(option)}>
        {#if option.create}
          <span class="mark"><Icon name="plus" /></span>{t("move.create", { folder: option.folder })}
        {:else}
          <span class="mark" aria-hidden="true">{option.folder.toLowerCase() === current.toLowerCase() ? "✓" : ""}</span>{label(option.folder)}
        {/if}
      </li>
    {/each}
  </ul>
</dialog>

<style>
dialog { box-sizing: border-box; width: calc(100dvw - var(--space-4) * 2); max-width: none; bottom: auto; margin: var(--space-2) auto 0; padding: var(--space-3); border: 1px solid var(--color-line); border-radius: var(--radius-sm); background: var(--color-bg); color: var(--color-fg); }
dialog::backdrop { background: var(--color-scrim); }
h2 { margin: 0 0 var(--space-2); font-size: var(--font-size-md); font-weight: 600; overflow-wrap: anywhere; }
dialog :global(input) { width: 100%; }
ul { list-style: none; margin: var(--space-2) 0 0; padding: 0; height: calc(var(--size-control) * 6); overflow: auto; }
li { box-sizing: border-box; display: flex; align-items: center; gap: var(--space-2); min-height: var(--size-control); padding: var(--space-1) var(--space-2); border-radius: var(--radius-sm); font-size: var(--font-size-sm); overflow-wrap: anywhere; cursor: default; }
li.active { background: var(--color-selected); color: var(--color-accent); }
.mark { display: inline-flex; flex: none; width: var(--size-icon); }
</style>
