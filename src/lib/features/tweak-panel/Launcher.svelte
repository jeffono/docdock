<script lang="ts">
import { devTweakWindow } from "../../ipc";
import Button from "../../ui/Button.svelte";
</script>

<div class="launcher"><Button variant="neutral" title="Open the tweak window (development builds only)" onclick={() => devTweakWindow().catch((e) => console.error("dev_tweak_window", e))}>Tweak</Button></div>

<style>
  .launcher { position: absolute; bottom: var(--space-2); right: var(--space-3); z-index: 4; }
</style>
