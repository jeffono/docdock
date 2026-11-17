<script lang="ts">
import { handleDefaults, handleFields, validHandle } from "./handle";
import NumberField from "./NumberField.svelte";
import { status, tweak } from "./state.svelte";

function set(key: (typeof handleFields)[number]["key"], value: number) {
  const next = { ...tweak.handle, [key]: value };
  if (!validHandle(next)) {
    status.error = `Invalid edge handle: ${handleFields.map(({ label, range }) => `${label} within ${range.join("..")}`).join(", ")}.`;
    return;
  }
  tweak.handle = next;
  status.error = "";
}
</script>

<details><summary>Edge handle</summary>
  <p>Applies from the next cursor move. Release builds keep the constants in handle.rs: {handleDefaults.activationPercent}% activation, {handleDefaults.revealPercent}% reveal radius, {handleDefaults.rushSpeed} px/s rush speed.</p>
  {#each handleFields as { key, label, range, step }}
    <label class="token">{label}<NumberField label={label} min={range[0]} max={range[1]} {step} value={tweak.handle[key]} onchange={(value) => set(key, value)} /></label>
  {/each}
</details>
