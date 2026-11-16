<script lang="ts">
import { handleDefaults, handleFields, validHandle } from "./handle";
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
  <p>Applies from the next cursor move. Release builds keep the constants in handle.rs: {handleDefaults.activationPercent}% activation, {handleDefaults.revealDistance} px reveal distance.</p>
  {#each handleFields as { key, label, range }}
    <label class="token">{label}<input type="number" min={range[0]} max={range[1]} step={key === "activationPercent" ? 10 : 1} value={tweak.handle[key]} onchange={(e) => set(key, e.currentTarget.valueAsNumber)} /></label>
  {/each}
</details>
