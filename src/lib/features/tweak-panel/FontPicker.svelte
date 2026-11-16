<script lang="ts">
import { onMount } from "svelte";
import { curatedFonts, fontStack, installedFonts } from "./fonts";

let {
  token,
  value,
  defaultValue,
  onchange,
}: { token: string; value: string; defaultValue: string; onchange: (value: string) => void } =
  $props();
let query = $state("");
let families = $state<string[]>([]);
let error = $state(false);
const choices = $derived([
  ...new Set([
    defaultValue,
    ...curatedFonts,
    ...families.map((family) => fontStack(family, token === "--font-mono")),
  ]),
]);
const matches = $derived(
  choices.filter((stack) =>
    stack.split(",")[0].toLocaleLowerCase().includes(query.toLocaleLowerCase()),
  ),
);
onMount(() => {
  void installedFonts().then(
    (names) => {
      families = names;
    },
    () => {
      error = true;
    },
  );
});
</script>

<input aria-label={`${"Search fonts"} ${token}`} placeholder={"Search fonts"} bind:value={query} />
<select aria-label={token} {value} onchange={(event) => onchange(event.currentTarget.value)}>
  {#if !matches.includes(value)}<option {value}>{value.split(",")[0]}</option>{/if}
  {#each matches as stack}<option value={stack}>{stack.split(",")[0]}</option>{/each}
</select>
{#if error}<small role="alert">{"Could not load installed fonts. Curated fonts remain available."}</small>{/if}

<style>
  input, select { min-width: 0; box-sizing: border-box; width: 100%; min-height: var(--size-control); border: 1px solid var(--color-line); border-radius: var(--radius-sm); padding: var(--space-1); background: var(--color-surface); color: var(--color-fg); font: inherit; }
  small { color: var(--color-danger); }
</style>
