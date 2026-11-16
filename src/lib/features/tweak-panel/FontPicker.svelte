<script lang="ts">
import { onMount } from "svelte";
import { curatedFonts, fontStack, installedFonts } from "./fonts";

let {
  name,
  value,
  initial,
  onchange,
}: { name: string; value: string; initial: string; onchange: (value: string) => void } = $props();

let query = $state("");
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
const matches = $derived(
  choices.filter((stack) =>
    stack.split(",")[0].toLocaleLowerCase().includes(query.toLocaleLowerCase()),
  ),
);
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

<input aria-label={`Search fonts ${name}`} placeholder="Search fonts" bind:value={query} />
<select aria-label={name} {value} onchange={(e) => onchange(e.currentTarget.value)}>
  {#if !matches.includes(value)}<option {value}>{value.split(",")[0]}</option>{/if}
  {#each matches as stack}<option value={stack}>{stack.split(",")[0]}</option>{/each}
</select>
{#if error}<small role="alert">Could not load installed fonts. Curated fonts remain available. <button type="button" onclick={load}>Retry</button></small>{/if}

<style>
  small[role="alert"] { color: var(--color-danger); }
</style>
