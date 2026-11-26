<script lang="ts">
import { onMount, tick } from "svelte";
import Notes from "./lib/features/notes/Notes.svelte";
import ShortcutSheet from "./lib/features/notes/ShortcutSheet.svelte";
import {
  activation,
  dismissActivationNotice,
  loadActivation,
  watchActivation,
} from "./lib/features/preferences/activation.svelte";
import Preferences from "./lib/features/preferences/Preferences.svelte";
import { loadTabWheelMode, tabWheelMode } from "./lib/features/preferences/tab-wheel-store";
import Scratchpad from "./lib/features/scratchpad/Scratchpad.svelte";
import EdgeTab from "./lib/features/tab/EdgeTab.svelte";
import { errorText, t } from "./lib/i18n";
import {
  exitReady,
  floatingEditCurrent,
  onExitCancelled,
  onExitRequested,
  onPanelEditNote,
  onPanelFocus,
  onPanelHidden,
  onPanelPinned,
  onPanelSearch,
  onPanelTabShape,
  onToggleMode,
  onWebState,
  onWebVisible,
  type PageArticle,
  type PageState,
  type PanelFocus,
  panelActivate,
  panelResize,
  panelSetPinned,
  panelTabShape,
  type TabShape,
  type TabSize,
  webHistory,
  webReader,
  webSetVisible,
} from "./lib/ipc";
import { createWheelStepper, type PanelTab, stepTab, wheelTabDelta } from "./lib/panel-tabs";
import IconButton from "./lib/ui/IconButton.svelte";
import Notice from "./lib/ui/Notice.svelte";

let notes: Notes;
let scratchpad: Scratchpad;
let shortcutSheet: ShortcutSheet;
let headerBar: HTMLElement;
/** The edge tab, drawn beside the panel once Rust gives its shape. */
let tab = $state<TabShape | null>(null);
/** The panel's own width, without the strip beside it where the tab shows. */
let width = $state(0);
let pinned = $state(false);
let activeTab = $state<PanelTab>("notes");
let previousTab: PanelTab = "notes";
let preferences = $state(false);
let page = $state<PageState | null>(null);
/** The reader view's article: the page window stays hidden while it is shown. */
let article = $state<PageArticle | null>(null);
/**
 * Changes when the reader view ends, the page is shown or hidden, or its URL changes, so an article
 * still being read is dropped. Rust drops it after a navigation to another document or a reload.
 */
let readerTurn = 0;
function closeReader() {
  article = null;
  readerTurn++;
}
let extracting = $state(false);
let exiting = false;
let error = $state("");
async function action(work: Promise<unknown>) {
  try {
    error = "";
    await work;
  } catch (e) {
    error = errorText(e);
  }
}
/** Rust reports it: polling also sees the page window, which DOM focus events cannot. */
let focus = $state<PanelFocus>({ focused: false, side: "right" });

$effect(() => {
  if (preferences || activeTab === "browser") shortcutSheet?.close();
});

let switching = false;
let targetTab: PanelTab | null = null;
let pendingSwitch: { next: PanelTab; shouldFocus: boolean } | null = null;
function stepCurrentTab(step: 1 | -1, shouldFocus: boolean) {
  const base = pendingSwitch?.next ?? targetTab ?? activeTab;
  void action(switchTab(stepTab(base, step), { shouldFocus }));
}

const wheelStepper = createWheelStepper();
function headerWheel(event: WheelEvent) {
  if (
    activation.recording ||
    exiting ||
    document.querySelector("dialog[open], [popover]:popover-open")
  )
    return;
  const delta = wheelTabDelta($tabWheelMode ?? "modifier", event);
  if (!delta) return;
  event.preventDefault();
  const step = wheelStepper.step(delta);
  if (!step) return;
  stepCurrentTab(step, focus.focused);
}

onMount(() => {
  let disposeDev: (() => void) | undefined;
  let disposed = false;
  if (import.meta.env.DEV) {
    void import("./lib/features/tweak-panel/install").then(({ install }) => {
      const target = document.querySelector("main");
      if (disposed || !target) return;
      disposeDev = install(target);
    });
  }
  const clipboardFailed = () => {
    error = t("error.clipboard");
  };
  window.addEventListener("clipboard-failed", clipboardFailed);
  // Not passive: prevented wheel over the top bar must never scroll inner content.
  headerBar.addEventListener("wheel", headerWheel, { passive: false });
  // The tab is drawn at the size Rust shaped the window to, never ahead of it. Listening starts
  // before the query, and a size reported meanwhile wins over the query's older answer.
  let reported: TabSize | undefined;
  const reshaped = onPanelTabShape((size) => {
    reported = size;
    if (tab) tab = { ...tab, ...size };
  });
  void reshaped
    .catch(() => {})
    .then(() => panelTabShape())
    .then((shape) => {
      tab = { ...shape, ...reported };
    })
    .catch(() => {});
  const activationChanged = watchActivation();
  void action(loadActivation());
  void loadTabWheelMode().catch(() => {});
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
        const notesSaved = await notes.prepareExit();
        const scratchpadSaved = notesSaved ? await scratchpad.prepareExit() : false;
        if (notesSaved && !scratchpadSaved) {
          closeReader();
          await webSetVisible(false);
          activeTab = "scratchpad";
          preferences = false;
        }
        saved = notesSaved && scratchpadSaved;
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
  const unpinned = onPanelPinned((value) => {
    pinned = value;
  });
  const state = onWebState((value) => {
    // A route change keeps the document, which Rust's check misses.
    if (value?.url !== page?.url) readerTurn++;
    page = value;
    // The page window is gone.
    if (!value) closeReader();
  });
  const visible = onWebVisible((value) => {
    // Entering the reader view hides the page but stays in Browser.
    if (!value && article) return;
    // A page opened from the bar or a link replaces the reader view.
    closeReader();
    if (value) {
      activeTab = "browser";
      preferences = false;
    } else if (activeTab === "browser") {
      activeTab = "notes";
    }
  });
  const toggle = onToggleMode((step) => stepCurrentTab(step, true));
  // A post-it asked to edit its note; the panel already took focus. One from a folder no longer
  // chosen leaves the panel as it is.
  const editNote = onPanelEditNote((request) => {
    void action(
      floatingEditCurrent(request.epoch).then(async (current) => {
        if (!current) return;
        if (activeTab !== "notes" || preferences || article)
          await switchTab("notes", { shouldFocus: true });
        await tick();
        await notes.editNote(request);
      }),
    );
  });
  const hidden = onPanelHidden(() => {
    shortcutSheet?.close();
    wheelStepper.reset();
  });
  const search = onPanelSearch((toggle) => {
    shortcutSheet?.close();
    if (preferences) {
      void action(
        switchTab("notes", { shouldFocus: true })
          .then(() => tick())
          .then(() => notes.focusSearch()),
      );
      return;
    }
    if (activeTab === "scratchpad") {
      if (toggle && scratchpad.containsFocus()) void action(scratchpad.handleEscape());
      else scratchpad.focus();
    }
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
      // An open menu or dialog takes its own Esc.
      !(
        event.target instanceof Element &&
        event.target.closest("[popover]:popover-open, dialog[open]")
      )
    ) {
      event.preventDefault();
      event.stopImmediatePropagation();
      void action(switchTab(previousTab, { shouldFocus: true }));
      return;
    }
    if (
      event.isComposing ||
      !event.ctrlKey ||
      event.altKey ||
      event.metaKey ||
      event.key !== "Tab" ||
      document.querySelector("dialog[open]")
    )
      return;
    event.preventDefault();
    event.stopPropagation();
    if (!event.repeat) {
      stepCurrentTab(event.shiftKey ? -1 : 1, true);
    }
  };
  window.addEventListener("keydown", shortcut, true);
  return () => {
    disposed = true;
    disposeDev?.();
    headerBar.removeEventListener("wheel", headerWheel);
    window.removeEventListener("clipboard-failed", clipboardFailed);
    window.removeEventListener("keydown", shortcut, true);
    for (const listener of [
      stop,
      unpinned,
      state,
      visible,
      toggle,
      hidden,
      search,
      editNote,
      exit,
      cancelled,
      activationChanged,
      reshaped,
    ])
      void listener.then((unlisten) => unlisten());
  };
});

async function switchTab(next: PanelTab, { shouldFocus = true }: { shouldFocus?: boolean } = {}) {
  if (switching) {
    pendingSwitch = { next, shouldFocus };
    return true;
  }
  switching = true;
  try {
    let current: { next: PanelTab; shouldFocus: boolean } | null = { next, shouldFocus };
    while (current) {
      pendingSwitch = null;
      const { next: target, shouldFocus: focusTarget } = current;
      targetTab = target;
      if (activeTab === "notes" && target !== "notes" && !(await notes.leaveEditor())) {
        pendingSwitch = null;
        return false;
      }
      if (activeTab === "scratchpad" && target !== "scratchpad") void scratchpad.flush();
      closeReader();
      const prev = activeTab;
      activeTab = target;
      preferences = false;
      if (target === "browser" || prev === "browser") await webSetVisible(target === "browser");
      await tick();
      if (!pendingSwitch) {
        if (focusTarget) {
          if (target === "browser") notes.focusSearch();
          else if (target === "scratchpad") scratchpad.focus();
        } else if (target === "scratchpad") {
          scratchpad.measure();
        }
      }
      current = pendingSwitch;
    }
    return true;
  } finally {
    targetTab = null;
    switching = false;
  }
}

async function togglePreferences() {
  if (preferences) return switchTab(previousTab, { shouldFocus: true });
  if (activeTab === "notes" && !(await notes.leaveEditor())) return false;
  if (activeTab === "scratchpad") void scratchpad.flush();
  previousTab = activeTab;
  closeReader();
  await webSetVisible(false);
  preferences = true;
  return true;
}

async function promoteFromScratchpad(
  content: string,
  afterCreate: () => Promise<void>,
): Promise<string | undefined> {
  const result = await notes.promoteNote(content);
  if ("blocked" in result) {
    await switchTab("notes", { shouldFocus: true });
    return;
  }
  if ("error" in result) return result.error;
  await afterCreate();
  await switchTab("notes", { shouldFocus: false });
  await notes.openPromoted(result.path);
}

/** Back and forward show the page they reach, leaving the reader view. */
async function history(back: boolean) {
  if (article) {
    closeReader();
    await webSetVisible(true);
  }
  await webHistory(back);
}
async function toggleReader() {
  if (article) {
    closeReader();
    return webSetVisible(true);
  }
  if (extracting) return;
  extracting = true;
  const turn = readerTurn;
  try {
    const found = await webReader();
    // Another page or mode was shown while the page was read.
    if (!found || turn !== readerTurn) return;
    article = found;
    await webSetVisible(false);
  } finally {
    extracting = false;
  }
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
  drag = { x: event.screenX, width };
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
  void action(panelResize(width + (event.key === wider ? step : -step), true));
}
function togglePin() {
  pinned = !pinned;
  void action(panelSetPinned(pinned));
}
</script>

<svelte:window
  onpointerdown={panelActivate}
  onblur={() => resizeEnd()}
  onkeydowncapture={(event) => {
    if (!preferences && activeTab !== "browser" && !notes?.isMovePickerOpen())
      shortcutSheet?.shortcut(event);
  }}
/>

<ShortcutSheet bind:this={shortcutSheet} />

{#if tab}<EdgeTab shape={tab} side={focus.side} focused={focus.focused} {pinned} />{/if}
<main
  class="panel"
  class:left={focus.side === "left"}
  style:--tab-depth={tab ? `${tab.depth}px` : undefined}
  bind:clientWidth={width}
>
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
  <header bind:this={headerBar}>
    <nav aria-label={t("app.mode")}>
      <button class:active={activeTab === "notes" && !preferences} aria-pressed={activeTab === "notes" && !preferences} title={t("web.notesShortcut")} onclick={() => action(switchTab("notes"))}>{t("web.notes")}</button>
      <button class:active={activeTab === "scratchpad" && !preferences} aria-pressed={activeTab === "scratchpad" && !preferences} title={t("web.scratchpadShortcut")} onclick={() => action(switchTab("scratchpad"))}>{t("web.scratchpad")}</button>
      <button class:active={activeTab === "browser" && !preferences} aria-pressed={activeTab === "browser" && !preferences} title={t("web.browserShortcut")} onclick={() => action(switchTab("browser"))}>{t("web.browser")}</button>
    </nav>
    {#if activeTab === "browser" && !preferences}
      <IconButton quiet icon="arrowLeft" label={t("web.back")} disabled={!page?.canGoBack} onclick={() => action(history(true))} />
      <IconButton quiet icon="arrowRight" label={t("web.forward")} disabled={!page?.canGoForward} onclick={() => action(history(false))} />
      <IconButton quiet icon="reader" label={t("web.reader")} pressed={article !== null} disabled={!page?.url || extracting} onclick={() => action(toggleReader())} />
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
  {#if preferences}<Preferences side={focus.side} {width} choose={() => notes.choose()} />{/if}
  <Scratchpad
    bind:this={scratchpad}
    active={!preferences && activeTab === "scratchpad"}
    choose={() => notes.choose()}
    onfoldertrusted={() => notes.reloadFolderState()}
    onpromote={promoteFromScratchpad}
    onopenpage={(url) => notes.openUrl(url)}
  />
  <Notes
    bind:this={notes}
    bind:article
    browser={activeTab === "browser"}
    inactive={preferences || activeTab === "scratchpad"}
    pageUrl={page?.url ?? ""}
    shortcutOpen={() => shortcutSheet?.isOpen() ?? false}
    beforeChoose={async () => {
      const ok = await scratchpad.prepareFolderSwitch();
      if (!ok) {
        closeReader();
        await webSetVisible(false);
        activeTab = "scratchpad";
        preferences = false;
      }
      return ok;
    }}
    afterChoose={() => scratchpad.finishFolderSwitch()}
    onfolderchange={() => scratchpad.reloadFolder()}
  />
</main>

<style>
  /*
   * The window reaches beyond the panel on its inner side by the edge tab's depth, a strip where
   * only the tab shows: the panel keeps to its side of it, and what the viewport would place, such
   * as menus and dialogs, keeps to the panel through `--frame-left`, `--frame-right` and
   * `--panel-width`. The tab draws the inner edge's line and focus band.
   */
  :global(body:has(> main.panel)) {
    background: transparent;
  }

  main {
    --frame-left: var(--tab-depth, 0px);
    --frame-right: 0px;
    --panel-width: calc(100dvw - var(--frame-left) - var(--frame-right));
    --panel-inset-left: calc(var(--space-4) + var(--space-2));
    --panel-inset-right: var(--space-3);
    position: relative;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    height: 100%;
    margin: 0 var(--frame-right) 0 var(--frame-left);
    background: var(--color-bg);
  }

  main.left {
    --frame-left: 0px;
    --frame-right: var(--tab-depth, 0px);
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
