<script lang="ts">
import { onMount } from "svelte";
import Notes from "./lib/features/notes/Notes.svelte";
import { errorText, t } from "./lib/i18n";
import {
  onPanelFocus,
  onToggleMode,
  onWebState,
  onWebVisible,
  type PageState,
  type PanelFocus,
  panelActivate,
  panelSetPinned,
  webHistory,
  webSetVisible,
} from "./lib/ipc";

import IconButton from "./lib/ui/IconButton.svelte";

let notes: Notes;
let pinned = $state(false);
let web = $state(false);
let page = $state<PageState | null>(null);
let error = $state("");
async function action(work: Promise<void>) {
  try {
    error = "";
    await work;
  } catch (e) {
    error = errorText(e);
  }
}
/** Rust reports it: polling also sees the page window, which DOM focus events cannot. */
let focus = $state<PanelFocus>({ focused: false, side: "right" });

onMount(() => {
  const stop = onPanelFocus((value) => {
    focus = value;
  });
  const state = onWebState((value) => {
    page = value;
  });
  const visible = onWebVisible((value) => {
    web = value;
  });
  const toggle = onToggleMode(() => void action(switchMode(!web)));
  const shortcut = (event: KeyboardEvent) => {
    if (
      event.isComposing ||
      !event.ctrlKey ||
      event.altKey ||
      event.shiftKey ||
      event.metaKey ||
      event.key !== "Tab"
    )
      return;
    event.preventDefault();
    event.stopPropagation();
    if (!event.repeat) void action(switchMode(!web));
  };
  window.addEventListener("keydown", shortcut, true);
  return () => {
    window.removeEventListener("keydown", shortcut, true);
    for (const listener of [stop, state, visible, toggle])
      void listener.then((unlisten) => unlisten());
  };
});

async function switchMode(value: boolean) {
  if (!(await notes.leaveEditor())) return;
  await webSetVisible(value);
  web = value;
  if (value) notes.focusSearch();
}
function togglePin() {
  pinned = !pinned;
  panelSetPinned(pinned);
}
</script>

<svelte:window onpointerdown={panelActivate} />

<main class:focused={focus.focused} class:left={focus.side === "left"}>
  <header>
    <nav aria-label={t("app.mode")}>
      <button class:active={!web} aria-pressed={!web} title={t("web.notesShortcut")} onclick={() => action(switchMode(false))}>{t("web.notes")}</button>
      <button class:active={web} aria-pressed={web} title={t("web.browserShortcut")} onclick={() => action(switchMode(true))}>{t("web.browser")}</button>
    </nav>
    {#if web}
      <IconButton quiet icon="arrowLeft" label={t("web.back")} disabled={!page?.canGoBack} onclick={() => action(webHistory(true))} />
      <IconButton quiet icon="arrowRight" label={t("web.forward")} disabled={!page?.canGoForward} onclick={() => action(webHistory(false))} />
    {/if}
    <span class="spacer"></span>
    <IconButton quiet icon="folder" label={t("notes.choose")} onclick={() => notes.choose()} />
    <IconButton quiet icon="pin" label={t("app.pin")} pressed={pinned} onclick={togglePin} />
  </header>
  {#if error}<p role="alert">{error}</p>{/if}
  <Notes bind:this={notes} browser={web} pageUrl={page?.url ?? ""} />
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

  p {
    color: var(--color-danger);
    margin: 0 var(--space-3);
  }

  nav { display: flex; gap: var(--space-1); }
  nav button {
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-muted);
    padding: var(--space-1) var(--space-2);
    font: inherit;
  }
  nav button.active { background: var(--color-surface); color: var(--color-fg); font-weight: 600; }
  header :global(button[aria-pressed="true"]) { background: var(--color-surface); color: var(--color-fg); }
  .spacer { flex: 1; }
</style>
