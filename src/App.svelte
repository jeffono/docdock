<script lang="ts">
import { panelActivate, panelSetEditing, panelSetPinned, webClose, webOpen } from "./lib/ipc";

let pinned = $state(false);
let url = $state("https://github.com");
let webError = $state("");

async function openPage(event: SubmitEvent) {
  event.preventDefault();
  try {
    webError = "";
    await webOpen(url);
  } catch (error) {
    webError = String(error);
  }
}

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
  <form onsubmit={openPage}>
    <input
      bind:value={url}
      aria-label="Endereço da página"
      onfocus={() => panelSetEditing(true)}
      onblur={() => panelSetEditing(false)}
      onkeydown={leaveField}
    />
    <button type="submit">Abrir</button>
    <button type="button" onclick={webClose}>Fechar página</button>
  </form>
  {#if webError}
    <p>{webError}</p>
  {/if}
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

  form {
    display: flex;
    gap: var(--space-2);
  }

  input {
    flex: 1;
    min-width: 0;
  }

  p {
    margin: 0;
    font-size: var(--font-size-sm);
  }

  textarea {
    flex: 1;
    resize: none;
    font-family: var(--font-sans);
  }
</style>
