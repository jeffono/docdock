<script lang="ts">
import { onMount } from "svelte";
import Notes from "./lib/features/notes/Notes.svelte";
import { errorText, t } from "./lib/i18n";
import {
  onPanelFocus,
  onWebState,
  onWebVisible,
  type PageState,
  type PanelFocus,
  panelActivate,
  panelSetPinned,
  webHistory,
  webSetVisible,
} from "./lib/ipc";
import Button from "./lib/ui/Button.svelte";
import IconButton from "./lib/ui/IconButton.svelte";

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
  return () => {
    for (const listener of [stop, state, visible]) void listener.then((unlisten) => unlisten());
  };
});

function togglePin() {
  pinned = !pinned;
  panelSetPinned(pinned);
}
</script>

<svelte:window onpointerdown={panelActivate} />

<main class:focused={focus.focused} class:left={focus.side === "left"}>
  <header>
    {#if web}
      <Button onclick={() => action(webSetVisible(false))}>{t("web.notes")}</Button>
      <IconButton icon="arrowLeft" label={t("web.back")} disabled={!page?.canGoBack} onclick={() => action(webHistory(true))} />
      <IconButton icon="arrowRight" label={t("web.forward")} disabled={!page?.canGoForward} onclick={() => action(webHistory(false))} />
      <h1 title={page?.url}>{page?.title || page?.url || t("web.address")}</h1>
    {:else}
      <h1>DocDock</h1>
      {#if page}<Button onclick={() => action(webSetVisible(true))}>{t("web.page")}</Button>{/if}
    {/if}
    <IconButton icon="pin" label={t("app.pin")} pressed={pinned} onclick={togglePin} />
  </header>
  {#if error}<p role="alert">{error}</p>{/if}
  <Notes browser={web} pageUrl={page?.url ?? ""} />
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

  h1 {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: 600;
    letter-spacing: var(--tracking-tight);
  }
</style>
