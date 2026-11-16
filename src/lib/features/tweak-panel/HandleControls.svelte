<script module lang="ts">
let settings = { percent: 300, reveal: 48 };
</script>

<script lang="ts">
import { devHandle } from "../../ipc";

let percent = $state(settings.percent);
let reveal = $state(settings.reveal);
let error = $state("");
async function apply() {
  try {
    await devHandle(percent, reveal);
    settings = { percent, reveal };
    error = "";
  } catch {
    error = "Use 100-1000% activation and 1-500 logical pixels for reveal.";
  }
}
</script>

<details>
  <summary>Edge handle</summary>
  <p>Live until restart. Release defaults: 300% activation, 48 logical pixels reveal distance.</p>
  <label>Activation height (%)<input type="number" min="100" max="1000" step="10" bind:value={percent} onchange={apply} /></label>
  <label>Reveal distance (logical px)<input type="number" min="1" max="500" bind:value={reveal} onchange={apply} /></label>
  {#if error}<p role="alert">{error}</p>{/if}
</details>

<style>
  details { border-top: 1px solid var(--color-line); padding: var(--space-2) 0; }
  summary { cursor: pointer; }
  p { color: var(--color-muted); margin: var(--space-2) 0; }
  label { display: grid; gap: var(--space-1); margin: var(--space-2) 0; }
  input { min-width: 0; box-sizing: border-box; width: 100%; min-height: var(--size-control); border: 1px solid var(--color-line); border-radius: var(--radius-sm); padding: var(--space-1); background: var(--color-surface); color: var(--color-fg); font: inherit; }
  [role="alert"] { color: var(--color-danger); }
</style>
