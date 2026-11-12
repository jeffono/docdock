<script lang="ts">
import { onMount } from "svelte";
import { dateTime, errorText, type MessageKey, t } from "../../i18n";
import {
  appAutostart,
  appSetAutostart,
  type NotesFolder,
  notesFolder,
  notesTrash,
  notesTrashEmpty,
  notesTrashRestore,
  onAutostartChanged,
  onNotesChanged,
  type PanelFocus,
  panelResize,
  panelSetSide,
  type TrashItem,
  vaultChoose,
  vaultClear,
} from "../../ipc";
import { appearance, chooseTheme } from "../../theme.svelte";
import Button from "../../ui/Button.svelte";
import ConfirmDialog from "../../ui/ConfirmDialog.svelte";
import { folderNotice } from "../notes/folder-state";
import NoteOrder from "../notes/NoteOrder.svelte";
import { noteOrderBusy, relativeDates, setRelativeDates } from "../notes/order-store";
import { relativeDate } from "../notes/relative-date";
import ShortcutList from "../notes/ShortcutList.svelte";
import ActivationShortcut from "./ActivationShortcut.svelte";
import UnlockVault from "./UnlockVault.svelte";
import { loadVault, vaultPath } from "./vault-store";

let { side, choose }: { side: PanelFocus["side"]; choose: () => Promise<string | undefined> } =
  $props();
const sections = ["general", "appearance", "browser", "shortcuts", "trash"] as const;
const themes = ["system", "light", "dark"] as const;
/** The panel's own width is the setting's current value. */
let width = $state(window.innerWidth);
let section = $state<(typeof sections)[number]>("general");
let folder = $state<NotesFolder | null>(null);
let error = $state("");
let busy = $state(false);
let autostart = $state<boolean | null>(null);
let confirm: ConfirmDialog;
/** The trash inside the notes folder; null while the folder is away. */
let trash = $state<TrashItem[] | null>([]);
/** Where the last restored item went. */
let restored = $state("");
const away = ["noFolder", "folderUnavailable", "folderMissing", "folderUnrecognized"];
/** Bumped when the list is dropped, so an answer still on its way, or a restore or empty that
 * started before, brings no names back. */
let trashLoads = 0;
async function loadTrash() {
  const load = ++trashLoads;
  try {
    const items = await notesTrash();
    if (load === trashLoads) trash = items;
  } catch (e) {
    if (!away.includes((e as { code?: string }).code ?? "")) throw e;
    if (load === trashLoads) trash = null;
  }
}
/** Note names stay out of memory outside the Trash section and while the folder is away (0009). */
function forgetTrash(items: TrashItem[] | null) {
  trashLoads++;
  trash = items;
  restored = "";
}
/** The item's name as the notes list shows it. */
function trashName({ path, folder }: TrashItem) {
  if (path === null) return t("trash.unrecognized");
  const name = path.slice(path.lastIndexOf("/") + 1);
  return folder ? name : name.replace(/\.md$/i, "");
}
/** Where it was; for an entry whose item cannot be told, where it is, to look at in File Explorer. */
function trashFolder({ id, path }: TrashItem) {
  if (path === null) return `.docdock/trash/${id}`;
  const at = path.lastIndexOf("/");
  return at < 0 ? t("trash.root") : path.slice(0, at);
}
async function restore(item: TrashItem) {
  restored = "";
  const load = trashLoads;
  await action(async () => {
    try {
      const path = await notesTrashRestore(item.id);
      if (load === trashLoads) restored = path;
    } finally {
      if (load === trashLoads) await loadTrash();
    }
  });
}
async function emptyTrash() {
  const ids = trash?.map((item) => item.id) ?? [];
  const count = ids.length;
  const text = count === 1 ? t("trash.emptyOne") : t("trash.emptyMany", { count });
  if (!(await confirm.ask(t("trash.emptyTitle"), text, t("trash.emptyConfirm"), undefined, true)))
    return;
  restored = "";
  const load = trashLoads;
  await action(async () => {
    try {
      await notesTrashEmpty(ids);
    } finally {
      if (load === trashLoads) await loadTrash();
    }
  });
}
$effect(() => {
  if (section !== "trash") return forgetTrash([]);
  restored = "";
  void action(loadTrash);
});
async function action(work: () => Promise<void>) {
  busy = true;
  error = "";
  try {
    await work();
  } catch (e) {
    error = errorText(e);
  } finally {
    busy = false;
  }
}
/** A failed choice leaves the theme as it was; the clicked radio must not stay checked. */
function resetTheme() {
  for (const input of document.querySelectorAll<HTMLInputElement>('input[name="theme"]'))
    input.checked = input.value === appearance.theme;
}
async function refreshFolder() {
  folder = await notesFolder();
}
onMount(() => {
  void action(async () => {
    await refreshFolder();
    await loadVault();
  });
  const notesChanged = onNotesChanged(() => {
    void refreshFolder()
      .then(() => {
        if (folder?.state !== "available") forgetTrash(null);
        else if (section === "trash") return loadTrash();
      })
      .catch((e) => {
        error = errorText(e);
      });
  });
  let disposed = false;
  let unlisten: (() => void) | undefined;
  void (async () => {
    const stop = await onAutostartChanged((enabled) => {
      autostart = enabled;
    });
    if (disposed) {
      stop();
      return;
    }
    unlisten = stop;
    const initial = await appAutostart();
    // A change reported while this read was in flight is newer.
    if (autostart === null) autostart = initial;
  })().catch((e) => {
    if (!disposed) error = errorText(e);
  });
  return () => {
    disposed = true;
    unlisten?.();
    void notesChanged.then((stop) => stop());
  };
});
</script>

<svelte:window bind:innerWidth={width} />

<ConfirmDialog bind:this={confirm} />

<section aria-label={t("preferences.title")}>
  <h1>{t("preferences.title")}</h1>
  <nav aria-label={t("preferences.title")}>
    {#each sections as item}
      <button class:active={section === item} aria-pressed={section === item} onclick={() => { section = item; }}>{t(`preferences.${item}`)}</button>
    {/each}
  </nav>
  {#if error}<p role="alert">{error}</p>{/if}
  <section class="settings" aria-label={t(`preferences.${section}`)}>
    {#if section === "general"}
      <div class="setting">
        <h3>{t("preferences.folder")}</h3>
        <p class="path">{folder?.path ?? t("notes.chooseHint")}</p>
        {#if folder && folder.state !== "available"}<p role="alert">{t(folderNotice(folder.state))}</p>{/if}
        {#if folder && folder.state !== "available"}<UnlockVault />{/if}
        <Button variant="neutral" disabled={busy} onclick={() => action(async () => { error = (await choose()) ?? ""; await refreshFolder(); })}>{t("notes.choose")}</Button>
      </div>
      <div class="setting">
        <h3>{t("vault.label")}</h3>
        <p class="path">{$vaultPath ?? t("vault.optional")}</p>
        <Button variant="neutral" disabled={busy} onclick={() => action(async () => { vaultPath.set(await vaultChoose()); })}>{t("vault.choose")}</Button>
        {#if $vaultPath}<Button variant="neutral" disabled={busy} onclick={() => action(async () => { await vaultClear(); vaultPath.set(null); })}>{t("vault.clear")}</Button>{/if}
      </div>
      <div class="setting"><h3>{t("order.label")}</h3><NoteOrder preferences /><p>{t("order.search")}</p></div>
      <div class="setting">
        <label><input type="checkbox" checked={$relativeDates} disabled={busy || $noteOrderBusy} onchange={(event) => { const enabled = event.currentTarget.checked; event.currentTarget.checked = $relativeDates; void action(() => setRelativeDates(enabled)); }} />{t("preferences.relativeDates")}</label>
      </div>
      <fieldset disabled={busy}>
        <legend>{t("preferences.side")}</legend>
        {#each ["left", "right"] as value}
          <label><input type="radio" name="side" checked={side === value} onchange={() => action(() => panelSetSide(value as PanelFocus["side"]))} />{t(`preferences.${value}` as MessageKey)}</label>
        {/each}
      </fieldset>
      <div class="setting">
        <label><input type="checkbox" checked={autostart === true} disabled={busy || autostart === null} onchange={(event) => { const enabled = event.currentTarget.checked; event.currentTarget.checked = autostart === true; void action(() => appSetAutostart(enabled)); }} />{t("preferences.autostart")}</label>
      </div>
    {:else if section === "appearance"}
      <fieldset disabled={busy}>
        <legend>{t("preferences.theme")}</legend>
        {#each themes as value}
          <label><input type="radio" name="theme" {value} checked={appearance.theme === value} onchange={() => action(() => chooseTheme(value).catch((e) => { resetTheme(); throw e; }))} />{t(`preferences.${value}`)}</label>
        {/each}
      </fieldset>
      <div class="setting">
        <h3>{t("preferences.width")}</h3>
        <p>{t("preferences.widthValue", { width })}</p>
        <p>{t("preferences.widthHint")}</p>
        <Button variant="neutral" disabled={busy} onclick={() => action(() => panelResize(null, true))}>{t("preferences.widthReset")}</Button>
      </div>
    {:else if section === "browser"}
      <div class="setting"><h3>{t("preferences.engine")}</h3><p>{t("preferences.engineName")}</p></div>
    {:else if section === "trash"}
      <div class="setting">
        <p>{t("trash.about")}</p>
        {#if trash === null}
          <p>{t("trash.unavailable")}</p>
        {:else if trash.length === 0}
          <p>{t("trash.empty")}</p>
        {:else}
          <ul class="trash">
            {#each trash as item (item.id)}
              <li>
                <div class="item">
                  <span class="name">{trashName(item)}</span>
                  <small>{trashFolder(item)} · <span title={dateTime(item.deleted)}>{relativeDate(item.deleted, Date.now(), document.documentElement.lang || "en")}</span></small>
                </div>
                {#if item.path !== null}<Button variant="neutral" disabled={busy} onclick={() => restore(item)}>{t("trash.restore")}</Button>{/if}
              </li>
            {/each}
          </ul>
          <Button variant="danger" disabled={busy} onclick={emptyTrash}>{t("trash.emptyAction")}</Button>
        {/if}
        {#if restored}<p class="restored" role="status">{t("trash.restored", { path: restored })}</p>{/if}
      </div>
    {:else}
      <ActivationShortcut />
      <ShortcutList />
    {/if}
  </section>
</section>

<style>
section { flex: 1; min-height: 0; overflow: auto; padding: var(--space-4) var(--panel-inset-right) var(--space-4) var(--panel-inset-left); }
h1 { font-size: var(--font-size-xl); margin: 0 0 var(--space-3); font-weight: 600; }
nav { display: flex; flex-wrap: wrap; gap: var(--space-1); padding-bottom: var(--space-3); border-bottom: 1px solid var(--color-line); }
nav button { font: inherit; color: var(--color-muted); background: transparent; border: 0; border-radius: var(--radius-sm); padding: var(--space-1) var(--space-2); }
nav button:hover, nav button.active { background: var(--color-surface); color: var(--color-fg); }
nav button.active { font-weight: 600; }
.settings { padding: var(--space-4) 0 0; overflow: visible; }
h3, legend { font-size: var(--font-size-md); font-weight: 600; margin: 0 0 var(--space-2); }
p { margin: 0 0 var(--space-3); color: var(--color-muted); }
p[role="alert"] { color: var(--color-danger); }
.path { overflow-wrap: anywhere; }
.setting { padding-bottom: var(--space-4); }
fieldset { border: 0; padding: 0; margin: var(--space-4) 0; }
label { display: inline-flex; align-items: center; gap: var(--space-1); margin-right: var(--space-4); }
input { accent-color: var(--color-accent); }
.trash { list-style: none; margin: 0 0 var(--space-3); padding: 0; border-top: 1px solid var(--color-line); }
.trash li { display: flex; align-items: center; gap: var(--space-2); padding: var(--space-2) 0; border-bottom: 1px solid var(--color-line); }
.item { display: flex; flex: 1; flex-direction: column; min-width: 0; }
.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--color-muted); font-size: var(--font-size-sm); }
.restored { margin-top: var(--space-3); overflow-wrap: anywhere; }
</style>
