<script lang="ts">
import { panelActivate, panelSetEditing, panelSetPinned } from "./lib/ipc";

let pinned = $state(false);

function togglePin() {
  pinned = !pinned;
  panelSetPinned(pinned);
}

function leaveField(event: KeyboardEvent) {
  if (event.key === "Escape") {
    (event.currentTarget as HTMLElement).blur();
  }
}
</script>

<svelte:window onpointerdown={panelActivate} />

<main>
  <h1>DocDock</h1>
  <button onclick={togglePin}>{pinned ? "Desafixar" : "Fixar"}</button>
  <textarea
    placeholder="Clique para digitar; Esc sai do campo"
    onfocus={() => panelSetEditing(true)}
    onblur={() => panelSetEditing(false)}
    onkeydown={leaveField}
  ></textarea>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    height: 100vh;
    box-sizing: border-box;
    padding: var(--space-4);
  }

  h1 {
    margin: 0;
    font-size: var(--font-size-lg);
  }

  textarea {
    flex: 1;
    resize: none;
    font-family: var(--font-sans);
  }
</style>
