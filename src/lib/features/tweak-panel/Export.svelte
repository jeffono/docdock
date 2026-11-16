<script lang="ts">
import Button from "../../ui/Button.svelte";
import { exportHandle } from "./handle";
import { exportMotion } from "./motion";
import { tweak } from "./state.svelte";
import { exportDiff } from "./tokens";

let shown = $state(false);
// One diff per source file; a section that tweaks another file adds its own here.
const diff = $derived.by(() => {
  if (!shown) return "";
  try {
    return [exportDiff(tweak.tokens), exportMotion(tweak.motion), exportHandle(tweak.handle)].join(
      "\n",
    );
  } catch (e) {
    return String(e);
  }
});
</script>

<Button variant="neutral" onclick={() => { shown = !shown; }}>{shown ? "Hide diffs" : "Export diffs"}</Button>
{#if diff}<textarea aria-label="Diffs to apply (select and copy)" readonly value={diff} onfocus={(e) => e.currentTarget.select()}></textarea>{/if}

<style>
  textarea { min-height: calc(var(--size-bar) * 6); margin-top: var(--space-2); font-family: var(--font-mono); }
</style>
