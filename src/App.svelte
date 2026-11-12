<script lang="ts">
import { onMount, tick } from "svelte";
import Notes from "./lib/features/notes/Notes.svelte";
import {
  activation,
  dismissActivationNotice,
  loadActivation,
  watchActivation,
} from "./lib/features/preferences/activation.svelte";
import Preferences from "./lib/features/preferences/Preferences.svelte";
import { errorText, t } from "./lib/i18n";
import {
  exitReady,
  onExitCancelled,
  onExitRequested,
  onPanelFocus,
  onPanelSearch,
  onToggleMode,
  onWebState,
  onWebVisible,
  type PageState,
  type PanelFocus,
  panelActivate,
  panelResize,
  panelSetPinned,
  webHistory,
  webSetVisible,
} from "./lib/ipc";

import IconButton from "./lib/ui/IconButton.svelte";
import Notice from "./lib/ui/Notice.svelte";

let notes: Notes;
let pinned = $state(false);
let web = $state(false);
let preferences = $state(false);
let previousWeb = false;
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
  const clipboardFailed = () => {
    error = t("error.clipboard");
  };
  window.addEventListener("clipboard-failed", clipboardFailed);
  const activationChanged = watchActivation();
  void action(loadActivation());
  let exiting = false;
  const cancelled = onExitCancelled(() => {
    exiting = false;
    document.body.inert = false;
  });
  const exit = onExitRequested((id) => {
    if (exiting) return;
    exiting = true;
    document.body.inert = true;
    void (async () => {
      let saved = false;
      try {
        saved = await notes.prepareExit();
        await exitReady(id, saved);
      } catch (e) {
        saved = false;
        error = errorText(e);
        await exitReady(id, false).catch(() => {});
      } finally {
        if (!saved) {
          exiting = false;
          document.body.inert = false;
        }
      }
    })();
  });
  const stop = onPanelFocus((value) => {
    focus = value;
  });
  const state = onWebState((value) => {
    page = value;
  });
  const visible = onWebVisible((value) => {
    web = value;
    if (value) preferences = false;
  });
  const toggle = onToggleMode(() => void action(switchMode(!web)));
  const search = onPanelSearch(() => {
    if (preferences)
      void action(
        switchMode(false)
          .then(() => tick())
          .then(() => notes.focusSearch()),
      );
  });
  const shortcut = (event: KeyboardEvent) => {
    if (activation.recording) return;
    if (
      !event.isComposing &&
      event.ctrlKey &&
      !event.shiftKey &&
      !event.altKey &&
      !event.metaKey &&
      !event.repeat &&
      event.key === ","
    ) {
      event.preventDefault();
      event.stopImmediatePropagation();
      void action(togglePreferences());
      return;
    }
    if (
      !event.isComposing &&
      event.ctrlKey &&
      event.shiftKey &&
      !event.altKey &&
      !event.metaKey &&
      event.key.toLowerCase() === "d"
    ) {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (!event.repeat) togglePin();
      return;
    }
    if (
      preferences &&
      event.key === "Escape" &&
      !event.isComposing &&
      !(event.target instanceof Element && event.target.closest("[popover]:popover-open"))
    ) {
      event.preventDefault();
      event.stopImmediatePropagation();
      void action(switchMode(previousWeb));
      return;
    }
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
    window.removeEventListener("clipboard-failed", clipboardFailed);
    window.removeEventListener("keydown", shortcut, true);
    for (const listener of [
      stop,
      state,
      visible,
      toggle,
      search,
      exit,
      cancelled,
      activationChanged,
    ])
      void listener.then((unlisten) => unlisten());
  };
});

async function switchMode(value: boolean) {
  if (!(await notes.leaveEditor())) return;
  await webSetVisible(value);
  web = value;
  preferences = false;
  if (value) notes.focusSearch();
}
async function togglePreferences() {
  if (preferences) return switchMode(previousWeb);
  if (!(await notes.leaveEditor())) return;
  previousWeb = web;
  await webSetVisible(false);
  web = false;
  preferences = true;
}
/** Dragging the inner edge: the window moves under the pointer, so only screen positions count. */
let drag: { x: number; width: number } | null = null;
let frame = 0;
let pendingWidth = 0;
function dragWidth(event: PointerEvent) {
  if (!drag) return 0;
  const moved = event.screenX - drag.x;
  return drag.width + (focus.side === "right" ? -moved : moved);
}
function resizeStart(event: PointerEvent) {
  if (event.button !== 0) return;
  event.preventDefault();
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  drag = { x: event.screenX, width: window.innerWidth };
  pendingWidth = 0;
}
function resizeMove(event: PointerEvent) {
  if (!drag) return;
  pendingWidth = dragWidth(event);
  // One resize per frame: each one moves two native windows.
  frame ||= requestAnimationFrame(() => {
    frame = 0;
    if (drag) void action(panelResize(pendingWidth, false));
  });
}
/** Idempotent: pointerup, a lost capture or a lost focus may each end the same drag. */
function resizeEnd(event?: PointerEvent) {
  if (!drag) return;
  const width = event?.type === "pointerup" ? dragWidth(event) : pendingWidth || drag.width;
  drag = null;
  cancelAnimationFrame(frame);
  frame = 0;
  void action(panelResize(width, true));
}
function resizeKey(event: KeyboardEvent) {
  const step = event.shiftKey ? 50 : 10;
  const wider = focus.side === "right" ? "ArrowLeft" : "ArrowRight";
  const narrower = focus.side === "right" ? "ArrowRight" : "ArrowLeft";
  if (event.key !== wider && event.key !== narrower) return;
  event.preventDefault();
  void action(panelResize(window.innerWidth + (event.key === wider ? step : -step), true));
}
function togglePin() {
  pinned = !pinned;
  panelSetPinned(pinned);
}
</script>

<svelte:window onpointerdown={panelActivate} onblur={() => resizeEnd()} />

<main class:focused={focus.focused} class:left={focus.side === "left"}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="resize"
    role="separator"
    aria-orientation="vertical"
    aria-label={t("app.resize")}
    title={t("app.resize")}
    tabindex="0"
    onpointerdown={resizeStart}
    onpointermove={resizeMove}
    onpointerup={resizeEnd}
    onpointercancel={() => resizeEnd()}
    onlostpointercapture={() => resizeEnd()}
    ondblclick={() => action(panelResize(null, true))}
    onkeydown={resizeKey}
  ></div>
  <header>
    <nav aria-label={t("app.mode")}>
      <button class:active={!web && !preferences} aria-pressed={!web && !preferences} title={t("web.notesShortcut")} onclick={() => action(switchMode(false))}>{t("web.notes")}</button>
      <button class:active={web} aria-pressed={web} title={t("web.browserShortcut")} onclick={() => action(switchMode(true))}>{t("web.browser")}</button>
    </nav>
    {#if web}
      <IconButton quiet icon="arrowLeft" label={t("web.back")} disabled={!page?.canGoBack} onclick={() => action(webHistory(true))} />
      <IconButton quiet icon="arrowRight" label={t("web.forward")} disabled={!page?.canGoForward} onclick={() => action(webHistory(false))} />
    {/if}
    <span class="spacer"></span>
    <IconButton quiet icon="settings" label={t("preferences.shortcutTooltip")} pressed={preferences} onclick={() => action(togglePreferences())} />
    <IconButton quiet icon="pin" label={t(pinned ? "app.unpinShortcut" : "app.pinShortcut")} pressed={pinned} onclick={togglePin} />
  </header>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if activation.failed}
    <Notice icon="alert">
      {t(activation.shortcut ? "notice.activationFailed" : "notice.activationNone", { failed: activation.failed, shortcut: activation.shortcut ?? "" })}
      {#snippet actions()}<IconButton quiet icon="x" label={t("search.close")} onclick={dismissActivationNotice} />{/snippet}
    </Notice>
  {/if}
  {#if preferences}<Preferences side={focus.side} choose={() => notes.choose()} />{/if}
  <Notes bind:this={notes} browser={web} inactive={preferences} pageUrl={page?.url ?? ""} />
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

  /* Over the inner edge, where the page window leaves a strip of the UI uncovered. */
  .resize {
    position: absolute;
    z-index: 3;
    top: 0;
    bottom: 0;
    left: 0;
    width: 6px;
    cursor: ew-resize;
    touch-action: none;
  }

  main.left .resize {
    right: 0;
    left: auto;
  }

  .resize:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
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
