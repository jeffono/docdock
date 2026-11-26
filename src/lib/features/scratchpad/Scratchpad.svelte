<script lang="ts">
import { onMount, tick } from "svelte";
import { errorText, type MessageKey, t } from "../../i18n";
import {
  clipboardWrite,
  type NotesFolder,
  notesFolder,
  notesTrust,
  type OpenNote,
  onPanelHidden,
  onScratchpadChanged,
  openExternal,
  panelClose,
  scratchpadOpen,
  scratchpadSave,
} from "../../ipc";
import Button from "../../ui/Button.svelte";
import Icon from "../../ui/Icon.svelte";
import Menu from "../../ui/Menu.svelte";
import { type Autosave, type AutosaveState, createAutosave } from "../notes/autosave";
import { type FolderGone, folderNotice } from "../notes/folder-state";
import { systemLink } from "../notes/link-actions";
import MarkdownEditor from "../notes/MarkdownEditor.svelte";
import UnlockVault from "../preferences/UnlockVault.svelte";
import { appendDraftEdit } from "./draft";
import { scratchpadSyncAction } from "./folder-sync";

let {
  active = false,
  choose,
  onfoldertrusted,
  onpromote,
  onopenpage,
}: {
  active?: boolean;
  choose: () => Promise<string | undefined>;
  onfoldertrusted: () => Promise<void>;
  onpromote: (content: string, afterCreate: () => Promise<void>) => Promise<string | undefined>;
  onopenpage: (url: string) => Promise<void>;
} = $props();

let folder = $state<NotesFolder | null>(null);
const ready = $derived(folder?.state === "available");
let opened = $state<OpenNote | null>(null);
let raw = $state("");
let version = "";
let status = $state<AutosaveState>("saved");
let error = $state("");
let resolving = $state(false);
let promoting = $state(false);
let folderSwitching = $state(false);
let updated = $state(false);
let copied = $state(false);
let copyTimer: ReturnType<typeof setTimeout> | undefined;
let folderGone = $state<FolderGone | null>(null);

const goneText: Record<FolderGone, MessageKey> = {
  unavailable: "editor.folderUnavailable",
  missing: "editor.folderMissing",
  unrecognized: "editor.folderUnrecognized",
};
const goneCodes: Record<string, FolderGone> = {
  folderUnavailable: "unavailable",
  folderMissing: "missing",
  folderUnrecognized: "unrecognized",
};

let editorCore = $state<MarkdownEditor>();
let container = $state<HTMLElement>();
let saving: Autosave | null = null;
let generation = 0;
let revision = 0;

function dirty(): boolean {
  return saving?.unsaved() !== null;
}

function failure(cause: unknown): string {
  const code = typeof cause === "object" && cause !== null && "code" in cause ? cause.code : null;
  const gone = Object.hasOwn(goneCodes, String(code)) ? goneCodes[String(code)] : undefined;
  if (gone) {
    folderGone = gone;
    return "";
  }
  return errorText(cause);
}

let linkMenu: Menu;
let menuLink = $state<{ href: string; url: URL }>();
function closeLinkMenu() {
  if (!menuLink) return;
  menuLink = undefined;
  linkMenu?.close();
}
async function openLinkMenu(
  event: MouseEvent | KeyboardEvent,
  href: string,
  from: HTMLElement,
  at?: { x: number; y: number },
) {
  const url = systemLink(href);
  if (!url) return;
  event.preventDefault();
  event.stopPropagation();
  if (!at) {
    const rect = from.getBoundingClientRect();
    at =
      event instanceof MouseEvent
        ? { x: event.clientX, y: event.clientY }
        : { x: rect.left, y: rect.bottom };
  }
  menuLink = { href, url };
  await tick();
  if (menuLink?.url === url && from.isConnected) linkMenu.show({ at, from });
}
function openSystemLink(url: URL) {
  void openExternal(url.href).catch((e) => {
    error = errorText(e);
  });
}

function clearState() {
  saving?.stop();
  saving = null;
  opened = null;
  raw = "";
  version = "";
  status = "saved";
  error = "";
  updated = false;
  folderGone = null;
  copied = false;
  clearTimeout(copyTimer);
  closeLinkMenu();
}

function setupAutosave(baseVersion: string) {
  saving?.stop();
  version = baseVersion;
  status = "saved";
  error = "";
  saving = createAutosave({
    version: baseVersion,
    save: async (text, base) => {
      const result = await scratchpadSave(text, base);
      if (result.status === "saved") version = result.version;
      return result;
    },
    onState: (next, cause) => {
      revision++;
      status = next;
      if (next === "saved") folderGone = null;
      error = cause ? failure(cause) : "";
    },
  });
}

async function loadFolder(value: NotesFolder | null) {
  const current = ++generation;
  folder = value;
  clearState();
  if (value?.state !== "available") return;
  try {
    const note = await scratchpadOpen();
    if (current !== generation) return;
    opened = note;
    raw = note.text;
    setupAutosave(note.version);
  } catch (e) {
    if (current === generation) error = failure(e);
  }
}

let reloading: Promise<void> | null = null;
let again = false;

function scheduleReload() {
  if (reloading) {
    again = true;
    return;
  }
  reloading = (async () => {
    do {
      again = false;
      try {
        await syncWithDisk();
      } catch (e) {
        if (opened) error = failure(e);
        else error = errorText(e);
      }
    } while (again);
    reloading = null;
  })();
}

async function syncWithDisk() {
  const current = generation;
  const editRevision = revision;
  const stale = () => {
    if (current !== generation) again = true;
    return current !== generation;
  };
  const value = await notesFolder();
  if (stale()) return;
  switch (scratchpadSyncAction(folder, folderGone, value, opened !== null, dirty())) {
    case "switch":
      await loadFolder(value);
      return;
    case "keep":
      generation++;
      folder = value;
      folderGone = value && value.state !== "available" ? value.state : "unavailable";
      error = "";
      revision++;
      closeLinkMenu();
      return;
    case "close":
    case "open":
      await loadFolder(value);
      return;
    case "resume":
      folder = value;
      folderGone = null;
      saving?.retry();
      break;
    case "reload":
      break;
  }
  if (!opened) {
    await loadFolder(value);
    return;
  }
  if (dirty() || resolving || promoting || status !== "saved") return;
  const note = await scratchpadOpen();
  if (
    stale() ||
    editRevision !== revision ||
    dirty() ||
    resolving ||
    promoting ||
    status !== "saved"
  )
    return;
  if (note.version !== version) {
    const changed = note.text !== raw;
    revision++;
    setupAutosave(note.version);
    editorCore?.reloadText(note.text);
    raw = note.text;
    updated = changed;
  }
}

async function resolve(keep: boolean) {
  if (resolving || promoting || !opened) return;
  resolving = true;
  try {
    const disk = await scratchpadOpen();
    if (keep) {
      saving?.stop();
      setupAutosave(disk.version);
      saving?.edit(raw);
      await saving?.flush();
    } else {
      revision++;
      setupAutosave(disk.version);
      editorCore?.reloadText(disk.text);
      raw = disk.text;
      updated = false;
    }
  } catch (e) {
    error = failure(e);
  } finally {
    resolving = false;
  }
}

async function discardChanges() {
  if (resolving || promoting) return;
  saving?.stop();
  if (folder?.state === "available") {
    await loadFolder(folder);
  } else {
    await loadFolder(await notesFolder().catch(() => folder));
  }
}

async function trustFolder() {
  try {
    error = "";
    const next = await notesTrust();
    if (opened && dirty()) {
      folder = next;
      folderGone = null;
      saving?.retry();
    } else if (opened && next && folder?.path === next.path && next.state === "available") {
      folder = next;
      folderGone = null;
      scheduleReload();
    } else {
      await loadFolder(next);
    }
    await onfoldertrusted();
  } catch (e) {
    error = errorText(e);
  }
}

async function chooseFolder() {
  error = (await choose()) ?? "";
}

async function copyAll() {
  if (!raw) return;
  try {
    error = "";
    await clipboardWrite(raw);
    copied = true;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => {
      copied = false;
    }, 1500);
  } catch (e) {
    error = errorText(e);
  }
}

function clearDraft() {
  if (!opened || resolving || promoting || folderSwitching) return;
  editorCore?.clear();
}

async function promote() {
  if (!opened || !raw.trim() || resolving || promoting || folderSwitching || folderGone) return;
  const content = raw;
  promoting = true;
  editorCore?.setReadOnly(true);
  error = "";
  try {
    const problem = await onpromote(content, async () => {
      if (raw === content && editorCore?.clear({ refocus: false, allowReadOnly: true })) {
        await saving?.flush();
      }
    });
    if (problem) error = problem;
  } catch (e) {
    error = failure(e);
  } finally {
    promoting = false;
    editorCore?.setReadOnly(folderSwitching);
  }
}

const saveText = $derived(
  status === "saved"
    ? t("editor.saved")
    : status === "saving" || status === "pending"
      ? t("editor.saving")
      : "",
);

$effect(() => {
  if (opened) editorCore?.setReadOnly(promoting || folderSwitching);
});

$effect(() => {
  if (active && opened) {
    requestAnimationFrame(() => editorCore?.measure());
  }
});

export function measure() {
  if (opened) editorCore?.measure();
}

export function focus() {
  void tick().then(() => {
    if (opened) {
      editorCore?.measure();
      editorCore?.focus();
    } else {
      container?.querySelector<HTMLElement>("button")?.focus({ preventScroll: true });
    }
  });
}

export function containsFocus(): boolean {
  const current = document.activeElement;
  return Boolean(editorCore?.hasFocus() || (current && container?.contains(current)));
}

export async function flush(): Promise<AutosaveState> {
  if (!opened || !saving) return "saved";
  return saving.flush();
}

export async function prepareExit(): Promise<boolean> {
  if (promoting || resolving || folderSwitching) {
    error = t("scratchpad.exitBusy");
    return false;
  }
  if (!opened) return true;
  return (await flush()) === "saved";
}

export async function prepareFolderSwitch(): Promise<boolean> {
  if (promoting || resolving) {
    error = t("scratchpad.exitBusy");
    return false;
  }
  folderSwitching = true;
  editorCore?.setReadOnly(true);
  if (!opened) return true;
  const saved = (await flush()) === "saved";
  if (!saved) {
    folderSwitching = false;
    editorCore?.setReadOnly(false);
  }
  return saved;
}

export function finishFolderSwitch() {
  folderSwitching = false;
  editorCore?.setReadOnly(promoting);
}

export async function reloadFolder() {
  scheduleReload();
  while (reloading) await reloading;
}

export function appendText(addition: string): string | undefined {
  if (resolving || promoting || folderSwitching) return t("scratchpad.exitBusy");
  if (folderGone) return t(goneText[folderGone]);
  if (!opened)
    return t(
      folder && folder.state !== "available"
        ? folderNotice(folder.state)
        : !folder
          ? "error.noFolder"
          : "scratchpad.exitBusy",
    );
  const edit = appendDraftEdit(raw, addition);
  if (!edit) return undefined;
  return editorCore?.appendText(edit) ? undefined : t("scratchpad.exitBusy");
}

export async function handleEscape() {
  if (!opened || (await flush()) === "saved") {
    await panelClose();
  }
}

onMount(() => {
  const scratchChanged = onScratchpadChanged(scheduleReload);
  const hidden = onPanelHidden(() => {
    closeLinkMenu();
    copied = false;
    clearTimeout(copyTimer);
    if (opened) void flush();
  });
  void notesFolder()
    .then(loadFolder)
    .catch((e) => {
      error = errorText(e);
    });
  return () => {
    clearTimeout(copyTimer);
    saving?.stop();
    void scratchChanged.then((stop) => stop());
    void hidden.then((stop) => stop());
  };
});
</script>

<svelte:window
  onkeydown={(event) => {
    if (
      active &&
      !event.defaultPrevented &&
      !event.isComposing &&
      event.key === "Escape" &&
      !document.querySelector("dialog[open], [popover]:popover-open")
    ) {
      event.preventDefault();
      void handleEscape();
    }
  }}
/>

<Menu bind:this={linkMenu} label={t("link.menu")} ontoggle={(open) => { if (!open) menuLink = undefined; }}>
  {#if menuLink}
    {@const target = menuLink}
    <button role="menuitem" tabindex="-1" onclick={() => openSystemLink(target.url)}>{t("link.openBrowser")}</button>
    {#if target.url.protocol !== "mailto:"}
      <button role="menuitem" tabindex="-1" onclick={() => void onopenpage(target.url.href).catch((e) => { error = errorText(e); })}>{t("link.openHere")}</button>
    {/if}
    <button role="menuitem" tabindex="-1" onclick={() => void clipboardWrite(target.href).catch((e) => { error = errorText(e); })}>{t("link.copy")}</button>
  {/if}
</Menu>

<section bind:this={container} class="scratchpad" hidden={!active} aria-label={t("scratchpad.label")}>
  {#if opened}
    <div class="tools">
      <button type="button" class="action" class:copied disabled={raw.length === 0} onclick={copyAll}>
        <Icon name={copied ? "check" : "copy"} />
        <span>{t(copied ? "scratchpad.copied" : "scratchpad.copyAll")}</span>
      </button>
      <button type="button" class="action" disabled={raw.length === 0 || resolving || promoting || folderSwitching} onclick={clearDraft}>
        <Icon name="eraser" />
        <span>{t("scratchpad.clear")}</span>
      </button>
      <button type="button" class="action" disabled={!raw.trim() || resolving || promoting || folderSwitching || folderGone !== null} onclick={promote}>
        <Icon name="filePlus" />
        <span>{t("scratchpad.promote")}</span>
      </button>
      {#if saveText && status !== "conflict" && status !== "error" && !folderGone}
        <small class="status" role="status">{saveText}</small>
      {/if}
    </div>
    <div class="editor-shell">
      {#if updated}
        <div class="notice" role="status">
          {t("editor.updated")}
          <button class="discard" onclick={() => editorCore?.undo()}>{t("editor.undo")}</button>
        </div>
      {/if}
      {#if status === "conflict"}
        <div class="notice" role="status">
          <p>{t("scratchpad.conflict")}</p>
          <div class="actions" hidden={folderGone !== null}>
            <Button onclick={() => resolve(true)} disabled={resolving}>{t("editor.keep")}</Button>
            <button class="discard" onclick={() => resolve(false)} disabled={resolving}>{t("editor.discard")}</button>
          </div>
        </div>
      {/if}
      {#if folderGone}
        <div class="notice" role="status">
          <p>{t(goneText[folderGone])}</p>
          <div class="actions">
            <UnlockVault />
            {#if folderGone === "unrecognized"}<Button onclick={trustFolder}>{t("notes.trust")}</Button>{/if}
            <button class="discard" onclick={discardChanges}>{t("editor.discard")}</button>
          </div>
        </div>
      {/if}
      {#if error}
        <div class="notice">
          <p class="error" role="alert">{error}</p>
          {#if status !== "conflict"}
            <div class="actions">
              <button class="discard" onclick={discardChanges}>{t("editor.discard")}</button>
            </div>
          {/if}
        </div>
      {/if}
      <MarkdownEditor
        bind:this={editorCore}
        initialText={opened.text}
        label={t("scratchpad.label")}
        placeholder={t("scratchpad.placeholder")}
        path={null}
        frontmatter={false}
        leaveOnCtrlE={false}
        fill
        autofocus={false}
        onleave={() => void handleEscape()}
        onblur={() => queueMicrotask(() => void saving?.flush())}
        onchange={(next) => {
          raw = next;
          updated = false;
          revision++;
          saving?.edit(raw);
        }}
        onlinkexternal={openSystemLink}
        onlinkmenu={openLinkMenu}
        onspellerror={(e) => {
          error = errorText(e);
        }}
      />
    </div>
  {:else if ready}
    {#if error}<p class="error standalone" role="alert">{error}</p>{/if}
  {:else}
    <div class="empty">
      <p>{folder && folder.state !== "available" ? t(folderNotice(folder.state)) : t("notes.chooseHint")}</p>
      {#if folder}<p class="path">{folder.path}</p>{/if}
      {#if folder}<UnlockVault />{/if}
      {#if folder?.state === "unrecognized"}<Button variant="neutral" onclick={trustFolder}>{t("notes.trust")}</Button>{/if}
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <Button variant="neutral" onclick={chooseFolder}>{t("notes.choose")}</Button>
    </div>
  {/if}
</section>

<style>
  section[hidden] {
    display: none;
  }

  .scratchpad {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }

  .tools {
    display: flex;
    flex: none;
    flex-wrap: wrap;
    gap: var(--space-1);
    align-items: center;
    box-sizing: border-box;
    min-height: var(--size-bar);
    padding: var(--space-1) var(--panel-inset-right) var(--space-1) var(--panel-inset-left);
    background: var(--color-bg);
  }

  .action {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--size-control);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-muted);
    font: inherit;
    font-size: var(--font-size-sm);
    white-space: nowrap;
    transition:
      background-color var(--duration),
      color var(--duration);
  }

  .action:hover:not(:disabled) {
    background: var(--color-surface);
    color: var(--color-fg);
  }

  .action.copied {
    color: var(--color-accent);
  }

  .action:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .status {
    margin-left: auto;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    white-space: nowrap;
  }

  .notice {
    flex: none;
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--color-line);
    font-size: var(--font-size-sm);
  }

  .notice p {
    margin: 0;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .discard {
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-fg);
    font: inherit;
    padding: 0 var(--space-2);
    min-height: var(--size-control);
  }

  .error {
    color: var(--color-danger);
    font-size: var(--font-size-sm);
    overflow-wrap: anywhere;
  }

  .error.standalone {
    margin: 0;
    padding: var(--space-4) var(--panel-inset-right) var(--space-4) var(--panel-inset-left);
  }

  .editor-shell {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    margin: 0 var(--panel-inset-right) var(--space-3) var(--panel-inset-left);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    --editor-pad-top: var(--space-3);
    --editor-pad-right: var(--space-3);
    --editor-pad-bottom: var(--space-3);
    --editor-pad-left: var(--space-3);
  }

  .empty {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    align-items: flex-start;
    padding: var(--space-4) var(--panel-inset-right) var(--space-4) var(--panel-inset-left);
  }

  .empty p {
    margin: 0;
    padding: 0;
  }

  .path {
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    overflow-wrap: anywhere;
  }

  @media (prefers-reduced-motion: reduce) {
    .action {
      transition: none;
    }
  }
</style>
