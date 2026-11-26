<script lang="ts">
import { onMount, tick } from "svelte";
import { dateTime, errorText, type MessageKey, t } from "../../i18n";
import {
  type ClipboardEntry,
  type ClipboardSnapshot,
  clipboardDeleteItem,
  clipboardOpenSettings,
  clipboardRestoreItem,
  clipboardSnapshot,
  clipboardWrite,
  onClipboardChanged,
  panelClose,
} from "../../ipc";
import {
  activeTransformCount,
  applyTransformState,
  classifyTransformControls,
  countActiveInControls,
  EMPTY_TRANSFORM_STATE,
  type ExclusiveSlot,
  isStructuredJson,
  isTransformActive,
  type TransformControlItem,
  type TransformId,
  type TransformState,
  toggleTransform,
} from "../../text-transforms";
import Button from "../../ui/Button.svelte";
import Icon from "../../ui/Icon.svelte";
import IconButton from "../../ui/IconButton.svelte";
import Notice from "../../ui/Notice.svelte";
import TextField from "../../ui/TextField.svelte";
import { countText } from "../notes/count";
import NoteStats from "../notes/NoteStats.svelte";
import { transformLabels } from "../notes/transforms";
import { filterClipboardEntries, previewSnippet } from "./filter";

let {
  active = false,
  onsendscratchpad,
  oncreatenote,
}: {
  active?: boolean;
  onsendscratchpad: (text: string) => Promise<string | undefined>;
  oncreatenote: (text: string) => Promise<string | undefined>;
} = $props();

let locale = $state(document.documentElement.lang || "en");
const numberFormat = $derived(new Intl.NumberFormat(locale));

let container: HTMLElement;
let listElement = $state<HTMLElement>();
let searchInput = $state<HTMLInputElement>();

let snapshot = $state<ClipboardSnapshot | null>(null);
let query = $state("");
let selectedId = $state<string | null>(null);
let expandedId = $state<string | null>(null);
let moreOpen = $state(false);
let transforms = $state<Record<string, TransformState>>({});
const sources = new Map<string, string>();

let copiedId = $state<string | null>(null);
let copyTimer: ReturnType<typeof setTimeout> | undefined;
let busy = $state(false);
let error = $state("");

type ClipboardItemView = {
  item: ClipboardEntry;
  state: TransformState;
  activeCount: number;
  text: string;
  snippet: string;
  isJson: boolean;
};

const filtered = $derived(filterClipboardEntries(snapshot?.items ?? [], query));
const itemsView = $derived<ClipboardItemView[]>(
  filtered.map((item) => {
    const state = item.truncated
      ? EMPTY_TRANSFORM_STATE
      : (transforms[item.id] ?? EMPTY_TRANSFORM_STATE);
    const activeCount = activeTransformCount(state);
    const text = activeCount ? applyTransformState(item.text, state, locale) : item.text;
    return {
      item,
      state,
      activeCount,
      text,
      snippet: previewSnippet(text),
      isJson: isStructuredJson(text),
    };
  }),
);
const expandedView = $derived(
  expandedId ? (itemsView.find((view) => view.item.id === expandedId) ?? null) : null,
);
const expandedLayout = $derived(
  expandedView && !expandedView.item.truncated
    ? classifyTransformControls(expandedView.item.text)
    : null,
);
const secondaryActiveCount = $derived(
  expandedView && expandedLayout
    ? countActiveInControls(expandedView.state, expandedLayout.secondary)
    : 0,
);
const expandedStats = $derived(expandedView ? countText(expandedView.text) : null);

const caseSymbols: Partial<Record<TransformId, string>> = {
  lowercase: "aa",
  "sentence-case": "Aa",
  "capitalize-words": "Aa Bb",
  uppercase: "AA",
};

const chipLabels: Partial<Record<TransformId, MessageKey>> = {
  "remove-empty-lines": "clipboard.chip.removeEmptyLines",
  "join-lines": "clipboard.chip.joinLines",
  "sort-ascending": "clipboard.chip.sortAscending",
  "sort-descending": "clipboard.chip.sortDescending",
  "digits-only": "clipboard.chip.digitsOnly",
  alphanumeric: "clipboard.chip.alphanumeric",
  "clean-url": "clipboard.chip.cleanUrl",
  "url-decode": "clipboard.chip.urlDecode",
  "normalize-whitespace": "clipboard.chip.normalizeWhitespace",
  "strip-accents": "clipboard.chip.stripAccents",
  "deduplicate-lines": "clipboard.chip.deduplicate",
  "format-json": "clipboard.chip.formatJson",
  "minify-json": "clipboard.chip.minifyJson",
};

const groupLabels: Record<ExclusiveSlot, MessageKey> = {
  case: "clipboard.group.case",
  json: "clipboard.group.json",
  lines: "clipboard.group.lines",
  sort: "clipboard.group.sort",
  extract: "clipboard.group.extract",
};

function shortLabel(id: TransformId): string {
  const symbol = caseSymbols[id];
  if (symbol) return symbol;
  const key = chipLabels[id];
  return t(key ?? transformLabels[id]);
}

function fullLabel(id: TransformId): string {
  return t(transformLabels[id]);
}

let refreshing: Promise<void> | null = null;
let refreshAgain = false;

async function refresh() {
  if (refreshing) {
    refreshAgain = true;
    return refreshing;
  }
  refreshing = (async () => {
    do {
      refreshAgain = false;
      try {
        const next = await clipboardSnapshot();
        const nextIds = new Set(next.items.map((item) => item.id));
        const nextTransforms: Record<string, TransformState> = {};
        for (const entry of next.items) {
          const prior = sources.get(entry.id);
          const existing = transforms[entry.id];
          if (prior === entry.text && existing && activeTransformCount(existing) > 0) {
            nextTransforms[entry.id] = existing;
          }
          sources.set(entry.id, entry.text);
        }
        for (const id of [...sources.keys()]) {
          if (!nextIds.has(id)) sources.delete(id);
        }
        transforms = nextTransforms;
        snapshot = next;
        if (expandedId && !nextIds.has(expandedId)) {
          expandedId = null;
          moreOpen = false;
        }
        if (selectedId && !nextIds.has(selectedId)) {
          selectedId = next.items[0]?.id ?? null;
        } else if (!selectedId && next.items.length > 0) {
          selectedId = next.items[0].id;
        }
        error = "";
      } catch (e) {
        error = errorText(e);
      }
    } while (refreshAgain);
    refreshing = null;
  })();
  return refreshing;
}

$effect(() => {
  if (active) {
    locale = document.documentElement.lang || "en";
    void refresh();
  } else {
    copiedId = null;
    clearTimeout(copyTimer);
  }
});

$effect(() => {
  const list = filtered;
  if (!list.length) {
    selectedId = null;
    return;
  }
  if (!selectedId || !list.some((item) => item.id === selectedId)) {
    selectedId = list[0].id;
  }
});

onMount(() => {
  const changed = onClipboardChanged(() => {
    if (active) void refresh();
  });
  return () => {
    clearTimeout(copyTimer);
    void changed.then((stop) => stop());
  };
});

function toggleExpand(item: ClipboardEntry) {
  selectedId = item.id;
  const next = expandedId === item.id ? null : item.id;
  if (next !== expandedId) moreOpen = false;
  expandedId = next;
}

function toggleItemTransform(id: string, transformId: TransformId) {
  if (snapshot?.items.find((entry) => entry.id === id)?.truncated) return;
  const current = transforms[id] ?? EMPTY_TRANSFORM_STATE;
  const next = toggleTransform(current, transformId);
  if (activeTransformCount(next) > 0) {
    transforms = { ...transforms, [id]: next };
  } else {
    const { [id]: _, ...rest } = transforms;
    transforms = rest;
  }
}

function resetTransforms(id: string) {
  if (!transforms[id]) return;
  const { [id]: _, ...rest } = transforms;
  transforms = rest;
}

async function copyItem(item: ClipboardEntry, text: string, transformed: boolean) {
  if (busy) return;
  busy = true;
  error = "";
  try {
    if (transformed) {
      await clipboardWrite(text);
    } else {
      const ok = await clipboardRestoreItem(item.id);
      if (!ok) {
        if (item.truncated) {
          error = t("clipboard.restoreUnavailable");
          return;
        }
        await clipboardWrite(text);
      }
    }
    copiedId = item.id;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => {
      if (copiedId === item.id) copiedId = null;
    }, 1500);
  } catch (e) {
    error = errorText(e);
  } finally {
    busy = false;
  }
}

async function deleteItem(item: ClipboardEntry) {
  if (!item.deletable || busy) return;
  busy = true;
  error = "";
  try {
    const list = filtered;
    const index = list.findIndex((entry) => entry.id === item.id);
    const nextFocusId = list[index + 1]?.id ?? list[index - 1]?.id ?? null;
    await clipboardDeleteItem(item.id);
    if (expandedId === item.id) expandedId = null;
    if (selectedId === item.id) selectedId = nextFocusId;
    resetTransforms(item.id);
    await refresh();
  } catch (e) {
    error = errorText(e);
  } finally {
    busy = false;
  }
}

async function sendToScratchpad(text: string) {
  if (busy) return;
  busy = true;
  error = "";
  try {
    const problem = await onsendscratchpad(text);
    if (problem) error = problem;
  } catch (e) {
    error = errorText(e);
  } finally {
    busy = false;
  }
}

async function createNote(text: string) {
  if (busy) return;
  busy = true;
  error = "";
  try {
    const problem = await oncreatenote(text);
    if (problem) error = problem;
  } catch (e) {
    error = errorText(e);
  } finally {
    busy = false;
  }
}

async function openSettings() {
  try {
    error = "";
    await clipboardOpenSettings();
  } catch (e) {
    error = errorText(e);
  }
}

function clearQuery() {
  query = "";
  searchInput?.focus();
}

function selectRelative(delta: number) {
  if (!filtered.length) return;
  const current = selectedId ? filtered.findIndex((entry) => entry.id === selectedId) : -1;
  const next =
    current < 0
      ? delta > 0
        ? 0
        : filtered.length - 1
      : Math.max(0, Math.min(filtered.length - 1, current + delta));
  selectedId = filtered[next].id;
  void tick().then(() => {
    container
      ?.querySelector<HTMLElement>(`.card[data-id="${CSS.escape(selectedId ?? "")}"]`)
      ?.scrollIntoView({ block: "nearest" });
  });
}

function onSearchKeydown(event: KeyboardEvent) {
  if (event.isComposing) return;
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    if (query) query = "";
    else void panelClose();
    return;
  }
  if (event.key === "ArrowDown" && itemsView.length) {
    event.preventDefault();
    if (!selectedId) selectedId = itemsView[0].item.id;
    listElement?.focus();
  } else if (event.key === "Enter" && itemsView.length) {
    event.preventDefault();
    const target = itemsView.find((view) => view.item.id === selectedId) ?? itemsView[0];
    selectedId = target.item.id;
    void copyItem(target.item, target.text, target.activeCount > 0);
  }
}

function onSectionKeydown(event: KeyboardEvent) {
  if (!active || event.isComposing) return;
  if (
    event.ctrlKey &&
    !event.shiftKey &&
    !event.altKey &&
    !event.metaKey &&
    event.key.toLowerCase() === "f"
  ) {
    event.preventDefault();
    event.stopPropagation();
    focusSearch();
    return;
  }
  const inInput =
    event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement;
  if (
    inInput ||
    document.querySelector("dialog[open], [popover]:popover-open") ||
    (event.target instanceof Element && event.target.closest("header"))
  )
    return;

  if (event.key === "ArrowDown") {
    event.preventDefault();
    selectRelative(1);
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    selectRelative(-1);
  } else if (event.key === "Delete") {
    if (container?.contains(document.activeElement)) {
      const current = itemsView.find((view) => view.item.id === selectedId);
      if (current?.item.deletable) {
        event.preventDefault();
        void deleteItem(current.item);
      }
    }
  } else if (event.key === "Escape") {
    event.preventDefault();
    if (query) {
      query = "";
      focusSearch();
    } else {
      void panelClose();
    }
  } else if (event.key === "Enter" && event.target === listElement) {
    const current = itemsView.find((view) => view.item.id === selectedId);
    if (current) {
      event.preventDefault();
      void copyItem(current.item, current.text, current.activeCount > 0);
    }
  }
}

export function focusSearch() {
  void tick().then(() => {
    searchInput?.focus();
    searchInput?.select();
  });
}

export function searchFocused(): boolean {
  return document.activeElement === searchInput;
}

export function reload() {
  void refresh();
}
</script>

<svelte:window onkeydown={onSectionKeydown} />

{#snippet renderControls(
  itemId: string,
  state: TransformState,
  controls: readonly TransformControlItem[],
)}
  {#each controls as control (control.kind === "segmented" ? control.slot : control.id)}
    {#if control.kind === "segmented"}
      <div class="segment-group" role="group" aria-label={t(groupLabels[control.slot])}>
        {#each control.options as opt (opt)}
          {@const activeOpt = isTransformActive(state, opt)}
          <button
            type="button"
            class="segment"
            data-transform={opt}
            aria-pressed={activeOpt}
            aria-label={fullLabel(opt)}
            title={fullLabel(opt)}
            onclick={() => toggleItemTransform(itemId, opt)}
          >
            {shortLabel(opt)}
          </button>
        {/each}
      </div>
    {:else}
      {@const activeToggle = isTransformActive(state, control.id)}
      <button
        type="button"
        class="toggle-chip"
        data-transform={control.id}
        aria-pressed={activeToggle}
        title={fullLabel(control.id)}
        onclick={() => toggleItemTransform(itemId, control.id)}
      >
        <span class="dot" aria-hidden="true"></span>
        <span>{shortLabel(control.id)}</span>
      </button>
    {/if}
  {/each}
{/snippet}

<section
  bind:this={container}
  class="clipboard-tab"
  hidden={!active}
  aria-label={t("clipboard.label")}
>
  <div class="tools">
    <div class="search-field">
      <TextField
        bind:ref={searchInput}
        placeholder={t("clipboard.searchPlaceholder")}
        aria-label={t("clipboard.search")}
        bind:value={query}
        onkeydown={onSearchKeydown}
      />
      {#if query}
        <div class="clear">
          <IconButton quiet icon="x" label={t("clipboard.clearSearch")} onclick={clearQuery} />
        </div>
      {/if}
    </div>
    {#if snapshot?.items.length}
      <small class="count" role="status">
        {t("clipboard.count", {
          count: numberFormat.format(filtered.length),
        })}
      </small>
    {/if}
  </div>

  {#if snapshot && !snapshot.historyEnabled && !snapshot.accessDenied}
    <Notice icon="clipboard">
      {t("clipboard.historyOff")}
      {#snippet actions()}
        <Button variant="neutral" onclick={() => void openSettings()}>
          {t("clipboard.openSettings")}
        </Button>
      {/snippet}
    </Notice>
  {:else if snapshot?.accessDenied}
    <Notice icon="alert">
      {t("clipboard.accessDenied")}
      {#snippet actions()}
        <Button variant="neutral" onclick={() => void refresh()}>
          {t("clipboard.retry")}
        </Button>
      {/snippet}
    </Notice>
  {/if}

  {#if snapshot?.currentNonText}
    <p class="hint" role="status">{t("clipboard.nonTextHint")}</p>
  {/if}

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  <div
    bind:this={listElement}
    class="list"
    tabindex="-1"
    role="region"
    aria-label={t("clipboard.itemsLabel")}
  >
    {#if itemsView.length === 0}
      <div class="empty">
        <p>
          {t(
            query.trim()
              ? "clipboard.noMatches"
              : snapshot?.historyEnabled
                ? "clipboard.emptyHistory"
                : "clipboard.emptyCurrent",
          )}
        </p>
      </div>
    {:else}
      {#each itemsView as view (view.item.id)}
        {@const item = view.item}
        {@const activeCount = view.activeCount}
        {@const text = view.text}
        {@const expanded = expandedId === item.id}
        {@const selected = selectedId === item.id}
        <article class="card" class:selected class:expanded data-id={item.id}>
          <header class="card-head">
            <button
              type="button"
              class="head-toggle"
              aria-expanded={expanded}
              onclick={() => toggleExpand(item)}
              ondblclick={() => void copyItem(item, text, activeCount > 0)}
            >
              <span class="preview" class:mono={view.isJson}>
                {view.snippet || t("clipboard.whitespaceOnly")}
              </span>
              <span class="meta">
                {#if activeCount > 0}
                  <span class="modified">{t("clipboard.transformedCount", { count: String(activeCount) })}</span>
                {/if}
                {#if item.timestampMs !== null}
                  <span>{dateTime(item.timestampMs)}</span>
                {/if}
                <span>{t("clipboard.chars", { count: numberFormat.format(text.length) })}</span>
              </span>
            </button>
            <div class="head-actions">
              <IconButton
                quiet
                icon={copiedId === item.id ? "check" : "copy"}
                label={t("clipboard.copy")}
                pressed={copiedId === item.id}
                onclick={() => void copyItem(item, text, activeCount > 0)}
              />
              {#if item.deletable}
                <IconButton
                  quiet
                  icon="trash"
                  label={t("clipboard.delete")}
                  onclick={() => void deleteItem(item)}
                />
              {/if}
            </div>
          </header>

          {#if expanded}
            <div class="details">
              <pre class="body" class:mono={view.isJson}>{text}</pre>
              {#if item.truncated}
                <p class="truncated">{t("clipboard.truncated")}</p>
              {/if}
              <div class="preview-bar">
                {#if activeCount > 0 && !item.truncated}
                  <button
                    type="button"
                    class="restore-original"
                    onclick={() => resetTransforms(item.id)}
                  >
                    <Icon name="restore" />
                    <span>{t("clipboard.restoreOriginal")}</span>
                  </button>
                {/if}
                {#if expandedStats}
                  <div class="stats">
                    <NoteStats total={{ words: expandedStats.words, chars: expandedStats.chars, images: 0 }} full={true} />
                  </div>
                {/if}
              </div>

              {#if !item.truncated && expandedLayout}
                <div class="transform-controls" role="group" aria-label={t("transform.menu")}>
                  {@render renderControls(item.id, view.state, expandedLayout.primary)}
                  {#if expandedLayout.secondary.length > 0}
                    <button
                      type="button"
                      class="more-toggle"
                      class:has-active={!moreOpen && secondaryActiveCount > 0}
                      aria-expanded={moreOpen}
                      onclick={() => {
                        moreOpen = !moreOpen;
                      }}
                    >
                      <span>
                        {moreOpen
                          ? t("clipboard.lessTransforms")
                          : secondaryActiveCount > 0
                            ? t("clipboard.moreTransformsCount", { count: String(secondaryActiveCount) })
                            : t("clipboard.moreTransforms")}
                      </span>
                      <Icon name="chevronDown" />
                    </button>
                  {/if}
                </div>

                {#if moreOpen && expandedLayout.secondary.length > 0}
                  <div class="transform-controls secondary">
                    {@render renderControls(item.id, view.state, expandedLayout.secondary)}
                  </div>
                {/if}
              {/if}

              <div class="action-row">
                <Button
                  disabled={busy}
                  onclick={() => void copyItem(item, text, activeCount > 0)}
                >
                  {t(
                    copiedId === item.id
                      ? "clipboard.copied"
                      : activeCount > 0
                        ? "clipboard.copyTransformed"
                        : "clipboard.copy",
                  )}
                </Button>
                <Button
                  variant="neutral"
                  disabled={busy || item.truncated}
                  onclick={() => void sendToScratchpad(text)}
                >
                  {t("clipboard.sendScratchpad")}
                </Button>
                <Button
                  variant="neutral"
                  disabled={busy || item.truncated}
                  onclick={() => void createNote(text)}
                >
                  {t("clipboard.createNote")}
                </Button>
              </div>
            </div>
          {/if}
        </article>
      {/each}
    {/if}
  </div>
</section>

<style>
  section[hidden] {
    display: none;
  }

  .clipboard-tab {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }

  .tools {
    display: flex;
    flex: none;
    gap: var(--space-2);
    align-items: center;
    box-sizing: border-box;
    min-height: var(--size-bar);
    padding: var(--space-1) var(--panel-inset-right) var(--space-1) var(--panel-inset-left);
  }

  .search-field {
    position: relative;
    display: flex;
    flex: 1;
    align-items: center;
    min-width: 0;
  }

  .search-field :global(input) {
    padding-right: calc(var(--size-control) + var(--space-1));
  }

  .clear {
    position: absolute;
    right: 1px;
    display: flex;
  }

  .count {
    flex: none;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    white-space: nowrap;
  }

  .hint,
  .error {
    flex: none;
    margin: 0;
    padding: 0 var(--panel-inset-right) var(--space-2) var(--panel-inset-left);
    font-size: var(--font-size-sm);
    overflow-wrap: anywhere;
  }

  .hint {
    color: var(--color-muted);
  }

  .error {
    color: var(--color-danger);
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-bottom: var(--space-3);
  }

  .list:focus-visible {
    outline: none;
  }

  .empty {
    padding: var(--space-4) var(--panel-inset-right) var(--space-4) var(--panel-inset-left);
    color: var(--color-muted);
  }

  .empty p {
    margin: 0;
  }

  .card {
    margin: 0 var(--panel-inset-right) var(--space-2) var(--panel-inset-left);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    min-width: 0;
  }

  .card.selected {
    border-color: var(--color-accent);
  }

  .card-head {
    display: flex;
    gap: var(--space-1);
    align-items: flex-start;
    padding: var(--space-1);
  }

  .card.expanded .card-head {
    border-bottom: 1px solid var(--color-line);
  }

  .head-toggle {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-fg);
    font: inherit;
    text-align: left;
  }

  .preview {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
    font-size: var(--font-size-md);
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }

  .modified {
    color: var(--color-accent);
    font-weight: 600;
  }

  .head-actions {
    display: flex;
    flex: none;
    gap: var(--space-1);
    align-items: center;
  }

  .card .head-actions :global(button) {
    opacity: 0;
    transition: opacity var(--duration);
  }

  .card:is(:hover, .selected, .expanded, :has(:focus-visible)) .head-actions :global(button),
  .head-actions :global(button[aria-pressed="true"]) {
    opacity: 1;
  }

  .details {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3) var(--space-3);
  }

  .body {
    margin: 0;
    padding: var(--space-2);
    max-height: 14rem;
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    font-family: inherit;
    font-size: var(--font-size-sm);
    line-height: var(--line-height);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .truncated {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }

  .preview-bar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
    min-height: var(--size-control);
  }

  .stats {
    margin-left: auto;
  }

  .stats :global(.count) {
    padding: 0;
  }

  .restore-original {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--size-control);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-accent);
    font: inherit;
    font-size: var(--font-size-sm);
    transition: background-color var(--duration);
  }

  .restore-original:hover {
    background: var(--color-selected);
  }

  .transform-controls,
  .action-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    align-items: center;
  }

  .transform-controls.secondary {
    padding-top: var(--space-2);
    border-top: 1px solid var(--color-line);
  }

  .action-row {
    gap: var(--space-2);
    padding-top: var(--space-1);
  }

  .segment-group {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: stretch;
    max-width: 100%;
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
  }

  .segment {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: var(--size-control);
    padding: 0 var(--space-2);
    border: 0;
    background: transparent;
    color: var(--color-fg);
    font: inherit;
    font-size: var(--font-size-sm);
    font-weight: 400;
    transition:
      background-color var(--duration),
      color var(--duration);
  }

  .segment:first-child {
    border-top-left-radius: calc(var(--radius-sm) - 1px);
    border-bottom-left-radius: calc(var(--radius-sm) - 1px);
  }

  .segment:last-child {
    border-top-right-radius: calc(var(--radius-sm) - 1px);
    border-bottom-right-radius: calc(var(--radius-sm) - 1px);
  }

  .segment + .segment {
    border-left: 1px solid var(--color-line);
  }

  .segment:hover {
    background: var(--color-selected);
  }

  .segment[aria-pressed="true"] {
    background: var(--color-selected);
    color: var(--color-accent);
  }

  .segment:focus-visible {
    position: relative;
    z-index: 1;
  }

  .toggle-chip,
  .more-toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--size-control);
    max-width: 100%;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-fg);
    font: inherit;
    font-size: var(--font-size-sm);
    font-weight: 400;
    transition:
      background-color var(--duration),
      border-color var(--duration),
      color var(--duration);
  }

  .toggle-chip:hover,
  .more-toggle:hover {
    background: var(--color-selected);
    border-color: var(--color-accent);
  }

  .toggle-chip[aria-pressed="true"] {
    background: var(--color-selected);
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .toggle-chip .dot {
    width: 0.375rem;
    height: 0.375rem;
    flex: none;
    box-sizing: border-box;
    border: 1px solid var(--color-muted);
    border-radius: var(--radius-full);
    transition:
      background-color var(--duration),
      border-color var(--duration);
  }

  .toggle-chip[aria-pressed="true"] .dot {
    border-color: var(--color-accent);
    background: var(--color-accent);
  }

  .more-toggle {
    color: var(--color-muted);
  }

  .more-toggle.has-active {
    background: var(--color-selected);
    border-color: var(--color-accent);
    color: var(--color-accent);
  }

  .more-toggle :global(svg) {
    transition: transform var(--duration);
  }

  .more-toggle[aria-expanded="true"] :global(svg) {
    transform: rotate(180deg);
  }

  @media (prefers-reduced-motion: reduce) {
    .card .head-actions :global(button),
    .restore-original,
    .segment,
    .toggle-chip,
    .toggle-chip .dot,
    .more-toggle,
    .more-toggle :global(svg) {
      transition: none;
    }
  }
</style>
