<script lang="ts">
import { onMount } from "svelte";
import { errorText, type MessageKey, t } from "../../i18n";
import {
  appAutostart,
  appSetAutostart,
  type NotesFolder,
  notesFolder,
  onAutostartChanged,
  onNotesChanged,
  type PanelFocus,
  panelResize,
  panelSetSide,
  vaultChoose,
  vaultClear,
} from "../../ipc";
import { appearance, chooseTheme } from "../../theme.svelte";
import Button from "../../ui/Button.svelte";
import { folderNotice } from "../notes/folder-state";
import NoteOrder from "../notes/NoteOrder.svelte";
import { noteOrderBusy, relativeDates, setRelativeDates } from "../notes/order-store";
import ShortcutList from "../notes/ShortcutList.svelte";
import ActivationShortcut from "./ActivationShortcut.svelte";
import UnlockVault from "./UnlockVault.svelte";
import { loadVault, vaultPath } from "./vault-store";

let { side, choose }: { side: PanelFocus["side"]; choose: () => Promise<string | undefined> } =
  $props();
const sections = ["general", "appearance", "browser", "shortcuts"] as const;
const themes = ["system", "light", "dark"] as const;
/** The panel's own width is the setting's current value. */
let width = $state(window.innerWidth);
let section = $state<(typeof sections)[number]>("general");
let folder = $state<NotesFolder | null>(null);
let error = $state("");
let busy = $state(false);
let autostart = $state<boolean | null>(null);
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
    void refreshFolder().catch((e) => {
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
    {:else}
      <ActivationShortcut />
      <ShortcutList />
    {/if}
  </section>
</section>

<style>
section { flex: 1; min-height: 0; overflow: auto; padding: var(--space-4) var(--panel-inset-right) var(--space-4) var(--panel-inset-left); }
h1 { font-size: var(--font-size-xl); margin: 0 0 var(--space-3); font-weight: 600; }
nav { display: flex; gap: var(--space-1); padding-bottom: var(--space-3); border-bottom: 1px solid var(--color-line); }
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
</style>
