<script module lang="ts">
let settings = { percent: 300, reveal: 48 };
</script>

<script lang="ts">
import { t } from "../../i18n";
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
    error = t("tweak.handleError");
  }
}
</script>

<details>
  <summary>{t("tweak.handle")}</summary>
  <p>{t("tweak.handleHint")}</p>
  <label>{t("tweak.activation")}<input type="number" min="100" max="1000" step="10" bind:value={percent} onchange={apply} /></label>
  <label>{t("tweak.reveal")}<input type="number" min="1" max="500" bind:value={reveal} onchange={apply} /></label>
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
