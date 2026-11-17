<script lang="ts">
import NumberField from "./NumberField.svelte";
import { status, tweak } from "./state.svelte";
import { tabDefaults, tabFields, validTab } from "./tab-shape";

function set(key: (typeof tabFields)[number]["key"], value: number) {
  const next = { ...tweak.tab, [key]: value };
  if (!validTab(next)) {
    status.error = `Invalid tab shape: ${tabFields.map(({ label, range }) => `${label} within ${range.join("..")}`).join(", ")}; the curve at most half the height.`;
    return;
  }
  tweak.tab = next;
  status.error = "";
}
</script>

<details><summary>Tab shape</summary>
  <p>Resizes the panel's window and its click region at the next cursor poll, open or parked. Release builds keep the constants in handle.rs ({tabDefaults.depth} depth, {tabDefaults.height} height, {tabDefaults.curve} curve) and the wordmark length in EdgeTab.svelte ({tabDefaults.wordmark}).</p>
  {#each tabFields as { key, label, range, step }}
    <label class="token">{label}<NumberField label={label} min={range[0]} max={range[1]} {step} value={tweak.tab[key]} onchange={(value) => set(key, value)} /></label>
  {/each}
</details>
