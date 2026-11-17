<script lang="ts">
import { onMount } from "svelte";
import { onTweakRequest, tweakSend } from "../../ipc";
import Contrast from "./Contrast.svelte";
import Export from "./Export.svelte";
import HandleControls from "./HandleControls.svelte";
import MotionSection from "./MotionSection.svelte";
import Presets from "./Presets.svelte";
import { payload, remember, restore, status, tweak } from "./state.svelte";
import TabShapeControls from "./TabShapeControls.svelte";
import TokenGroups from "./TokenGroups.svelte";

restore();

function send() {
  const state = payload($state.snapshot(tweak));
  remember(state);
  tweakSend(state).catch((e) => {
    status.error = `Could not reach the panel: ${e}`;
  });
}

// Every change goes to the panel whole: it keeps no tweak state of its own.
$effect(send);
onMount(() => {
  const request = onTweakRequest(send);
  return () => void request.then((unlisten) => unlisten());
});
</script>

<main>
  <h1>Tweaks · DEV</h1>
  <p>Applies live to the panel, even while it is hidden, which otherwise behaves as in production. Changes stay until the panel reloads, which asks this window again.</p>
  {#if status.error}<p role="alert">{status.error}</p>{/if}
  <div class="columns">
    <section class="options" aria-label="Tweak options">
      <TokenGroups /><MotionSection /><HandleControls /><TabShapeControls />
      <details><summary>Presets and export</summary><Presets /><Export /></details>
      <p>Bar height previews CSS only; the native page offset is fixed. Verify a chosen size in the app.</p>
    </section>
    <aside aria-label="WCAG contrast"><Contrast /></aside>
  </div>
</main>

<style>
  main { height: 100dvh; box-sizing: border-box; display: flex; flex-direction: column; min-height: 0; padding: var(--space-3); font-size: var(--font-size-sm); }
  .columns { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 3fr) minmax(0, 2fr); gap: var(--space-3); }
  .options, aside { overflow: auto; min-height: 0; padding: var(--space-1); }
  aside { border-left: 1px solid var(--color-line); }
  @media (max-width: 600px) { .columns { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 3fr) minmax(0, 2fr); } aside { border-left: 0; border-top: 1px solid var(--color-line); } }
  h1 { margin: 0 0 var(--space-2); font-size: var(--font-size-lg); }
  p { color: var(--color-muted); margin: var(--space-2) 0; }
  [role="alert"] { color: var(--color-danger); }
  main :global(details) { border-top: 1px solid var(--color-line); padding: var(--space-2) 0; }
  main :global(summary) { cursor: pointer; }
  main :global(.token) { display: grid; gap: var(--space-1); margin: var(--space-2) 0; }
  main :global(.token > span) { font-family: var(--font-mono); overflow-wrap: anywhere; }
  main :global(label) { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-1); }
  main :global(:is(input, select, textarea)) { min-width: 0; box-sizing: border-box; width: 100%; border: 1px solid var(--color-line); border-radius: var(--radius-sm); padding: var(--space-1); background: var(--color-surface); color: var(--color-fg); font: inherit; }
  main :global(:is(input, select)) { min-height: var(--size-control); }
  main :global(small) { font: inherit; color: var(--color-muted); }
</style>
