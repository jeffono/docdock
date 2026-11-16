<script lang="ts">
import { onMount } from "svelte";
import Button from "../../ui/Button.svelte";
import { load, payload, status, tweak } from "./state.svelte";

// Each preset holds edited tokens and all motions, in the format of the panel's payload.
const key = "docdock-tweak-presets-v2";
let presets = $state<Record<string, unknown>>({});
let name = $state("");
let selected = $state("");

onMount(() => {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(key) ?? "{}");
    if (stored && typeof stored === "object" && !Array.isArray(stored))
      presets = stored as Record<string, unknown>;
  } catch {
    status.error = "Could not read saved presets.";
  }
});

function save() {
  const label = name.trim();
  if (!label) return;
  const next = { ...presets, [label]: payload($state.snapshot(tweak)) };
  try {
    localStorage.setItem(key, JSON.stringify(next));
    presets = next;
    selected = label;
    status.error = "";
  } catch {
    status.error = "Could not save presets. Export the diff to keep these changes.";
  }
}
</script>

<div class="row">
  <select aria-label="Named preset" bind:value={selected} onchange={() => { if (selected in presets) load(presets[selected]); }}>
    <option value="">Choose preset</option>
    {#each Object.keys(presets) as preset}<option value={preset}>{preset}</option>{/each}
  </select>
  <Button variant="neutral" onclick={() => { load({}); selected = ""; }}>Reset</Button>
</div>
<div class="row"><input aria-label="Preset name" placeholder="Preset name (A / B)" bind:value={name} /><Button variant="neutral" onclick={save}>Save</Button></div>

<style>
  .row { display: flex; gap: var(--space-2); align-items: center; margin-bottom: var(--space-2); }
</style>
