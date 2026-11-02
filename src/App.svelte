<script lang="ts">
import Notes from "./lib/features/notes/Notes.svelte";
import WebBar from "./lib/features/web/WebBar.svelte";
import { panelActivate, panelSetPinned, webClose } from "./lib/ipc";
import IconButton from "./lib/ui/IconButton.svelte";

let pinned = $state(false);
let web = $state(false);

function togglePin() {
  pinned = !pinned;
  panelSetPinned(pinned);
}

function toggleWeb() {
  if (web) {
    webClose();
  }
  web = !web;
}
</script>

<svelte:window onpointerdown={panelActivate} />

<main>
  <header>
    <h1>DocDock</h1>
    <IconButton icon="globe" label="Página web" pressed={web} onclick={toggleWeb} />
    <IconButton icon="pin" label="Fixar painel" pressed={pinned} onclick={togglePin} />
  </header>
  {#if web}
    <WebBar />
  {/if}
  <Notes />
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    height: 100%;
    border-left: 1px solid var(--color-line);
  }

  header {
    display: flex;
    flex: none;
    gap: var(--space-2);
    align-items: center;
    box-sizing: border-box;
    height: var(--size-bar);
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--color-line);
  }

  h1 {
    flex: 1;
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: 600;
    letter-spacing: var(--tracking-tight);
  }
</style>
