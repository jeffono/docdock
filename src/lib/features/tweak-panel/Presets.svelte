<script lang="ts">
import { onMount } from "svelte";
import Button from "../../ui/Button.svelte";
import { type PresetAction, presetKey, updatePresets } from "./presets";
import { load, payload, status, tweak } from "./state.svelte";

let presets = $state<Record<string, unknown>>({});
let name = $state("");
let selected = $state("");
let pending = $state<"delete" | "overwrite" | null>(null);
onMount(() => {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(presetKey) ?? "{}");
    if (stored && typeof stored === "object" && !Array.isArray(stored))
      presets = stored as Record<string, unknown>;
  } catch {
    status.error = "Could not read saved presets.";
  }
});
function apply(action: PresetAction) {
  try {
    presets = updatePresets(localStorage, presets, action);
    selected =
      action.kind === "delete"
        ? ""
        : action.kind === "rename"
          ? action.next.trim()
          : action.name.trim();
    pending = null;
    status.error = "";
  } catch (e) {
    status.error = `Could not update presets: ${e}`;
  }
}
function save() {
  if (Object.hasOwn(presets, name.trim())) {
    selected = name.trim();
    pending = "overwrite";
  } else apply({ kind: "save", name, value: payload($state.snapshot(tweak)) });
}
function confirm() {
  if (pending === "delete") apply({ kind: "delete", name: selected });
  else
    apply({
      kind: "save",
      name: selected,
      value: payload($state.snapshot(tweak)),
      overwrite: true,
    });
}
</script>
<div class="row">
<select aria-label="Named preset" bind:value={selected} onchange={() => { pending = null; name = selected; if (Object.hasOwn(presets, selected)) load(presets[selected]); }}>
<option value="">Choose preset</option>{#each Object.keys(presets) as preset}<option value={preset}>{preset}</option>{/each}
</select><Button variant="neutral" onclick={() => { load({}); selected = ""; pending = null; }}>Reset</Button>
</div>
<div class="row"><input aria-label="Preset name" placeholder="Preset name (A / B)" bind:value={name} /><Button variant="neutral" onclick={save}>Save</Button></div>
<div class="row">
<Button variant="neutral" disabled={!selected} onclick={() => apply({kind: "rename", name: selected, next: name})}>Rename</Button>
<Button variant="neutral" disabled={!selected} onclick={() => { pending = "overwrite"; }}>Overwrite</Button>
<Button variant="neutral" disabled={!selected} onclick={() => { pending = "delete"; }}>Delete</Button>
</div>
{#if pending}<div role="group" aria-label="Confirm preset change"><p>{pending === "delete" ? "Delete" : "Overwrite"} preset &quot;{selected}&quot;?</p><div class="row"><Button variant="neutral" onclick={confirm}>Confirm {pending}</Button><Button variant="neutral" onclick={() => { pending = null; }}>Cancel</Button></div></div>{/if}
<style>
.row { display: flex; flex-wrap: wrap; gap: var(--space-2); align-items: center; margin: var(--space-2) 0; }
.row :global(input), .row :global(select) { flex: 1; width: auto; }
</style>
