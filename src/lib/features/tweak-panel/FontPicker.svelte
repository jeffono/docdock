<script lang="ts">
import { onMount } from "svelte";
import { curatedFonts, filterFonts, fontStack, installedFonts } from "./fonts";

let {
  name,
  value,
  initial,
  onchange,
}: { name: string; value: string; initial: string; onchange: (value: string) => void } = $props();

let query = $state("");
let open = $state(false);
let active = $state(0);
const listId = $props.id();
const title = (stack: string) => stack.split(",")[0].replaceAll('"', "");
function choose(stack: string) {
  onchange(stack);
  query = "";
  open = false;
}
function keydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    active = open
      ? Math.max(0, Math.min(matches.length - 1, active + (e.key === "ArrowDown" ? 1 : -1)))
      : 0;
    open = true;
    requestAnimationFrame(() =>
      document.getElementById(`${listId}-${active}`)?.scrollIntoView({ block: "nearest" }),
    );
  } else if (e.key === "Enter" && open && matches[active]) {
    e.preventDefault();
    choose(matches[active]);
  } else if (e.key === "Escape") {
    e.preventDefault();
    open = false;
    query = "";
  }
}
let families = $state<string[]>([]);
let error = $state(false);
// The source value and curated stacks first, then each installed family with a fallback.
const choices = $derived([
  ...new Set([
    initial,
    ...curatedFonts,
    ...families.map((family) => fontStack(family, name === "--font-mono")),
  ]),
]);
const matches = $derived(filterFonts(choices, query));
// A panel reloading when the window asked never answers: Retry asks again.
function load() {
  error = false;
  installedFonts().then(
    (names) => {
      families = names;
    },
    () => {
      error = true;
    },
  );
}
onMount(load);
</script>

<div class="combo">
<input role="combobox" aria-label={name} aria-autocomplete="list" aria-expanded={open} aria-controls={listId} aria-activedescendant={open && matches[active] ? `${listId}-${active}` : undefined} value={open ? query : title(value)} placeholder={title(value)} onfocus={() => { query = ""; open = true; active = 0; }} oninput={(e) => { query = e.currentTarget.value; open = true; active = 0; }} onblur={() => { open = false; query = ""; }} onkeydown={keydown} />
{#if open}
<div class="choices" id={listId} role="listbox" aria-label={name}>
{#each matches as stack, i}<div id={`${listId}-${i}`} role="option" tabindex="-1" aria-selected={i === active} onpointerdown={(e) => { e.preventDefault(); choose(stack); }}>{title(stack)}</div>{/each}
{#if !matches.length}<small>No matching fonts</small>{/if}
</div>
{/if}
</div>
{#if error}<small role="alert">Could not load installed fonts. Curated fonts remain available. <button type="button" onclick={load}>Retry</button></small>{/if}

<style>
  .combo { position: relative; }
  .choices { position: absolute; z-index: 1; width: 100%; max-height: calc(var(--size-bar) * 5); overflow: auto; border: 1px solid var(--color-line); background: var(--color-surface); box-sizing: border-box; }
  [role="option"] { padding: var(--space-1); cursor: pointer; }
  [aria-selected="true"] { background: var(--color-selected); }
  small[role="alert"] { color: var(--color-danger); }
</style>
