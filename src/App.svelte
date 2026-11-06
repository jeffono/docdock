<script lang="ts">
import { onMount } from "svelte";
import Notes from "./lib/features/notes/Notes.svelte";
import WebBar from "./lib/features/web/WebBar.svelte";
import { t } from "./lib/i18n";
import { onPanelFocus, type PanelFocus, panelActivate, panelSetPinned, webClose } from "./lib/ipc";
import IconButton from "./lib/ui/IconButton.svelte";

let pinned = $state(false);
let web = $state(false);
/** Rust reports it: polling also sees the page window, which DOM focus events cannot. */
let focus = $state<PanelFocus>({ focused: false, side: "right" });

onMount(() => {
  const stop = onPanelFocus((value) => {
    focus = value;
  });
  return () => void stop.then((unlisten) => unlisten());
});

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

<main class:focused={focus.focused} class:left={focus.side === "left"}>
  <header>
    <h1>DocDock</h1>
    <IconButton icon="globe" label={t("app.web")} pressed={web} onclick={toggleWeb} />
    <IconButton icon="pin" label={t("app.pin")} pressed={pinned} onclick={togglePin} />
  </header>
  {#if web}
    <WebBar />
  {/if}
  <Notes />
</main>

<style>
  main {
    --panel-inset-left: calc(var(--space-4) + var(--space-2));
    --panel-inset-right: var(--space-3);
    position: relative;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    height: 100%;
    border-left: 1px solid var(--color-line);
  }

  /* While the panel has focus: a band on the inner edge, over the border, above the content. */
  main.focused::before {
    content: "";
    position: absolute;
    z-index: 2;
    top: 0;
    bottom: 0;
    left: -1px;
    width: 2px;
    background: var(--color-accent);
    pointer-events: none;
  }

  main.focused.left::before {
    right: 0;
    left: auto;
  }

  header {
    display: flex;
    flex: none;
    gap: var(--space-2);
    align-items: center;
    box-sizing: border-box;
    height: var(--size-bar);
    padding: 0 var(--panel-inset-right) 0 var(--panel-inset-left);
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
