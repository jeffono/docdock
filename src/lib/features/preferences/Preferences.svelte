<script lang="ts">
import { onMount } from "svelte";
import { errorText, type MessageKey, t } from "../../i18n";
import {
  type NotesFolder,
  notesFolder,
  type PanelFocus,
  panelResize,
  panelSetSide,
} from "../../ipc";
import Button from "../../ui/Button.svelte";
import { formattingShortcuts } from "../notes/formatting";
import NoteOrder from "../notes/NoteOrder.svelte";

let { side, choose }: { side: PanelFocus["side"]; choose: () => Promise<string | undefined> } =
  $props();
const sections = ["general", "appearance", "browser", "shortcuts"] as const;
/** The panel's own width is the setting's current value. */
let width = $state(window.innerWidth);
let section = $state<(typeof sections)[number]>("general");
let folder = $state<NotesFolder | null>(null);
let error = $state("");
let busy = $state(false);
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
async function refreshFolder() {
  folder = await notesFolder();
}
onMount(() => {
  void action(refreshFolder);
});
const panelShortcuts: [string, MessageKey][] = [
  ["Ctrl+Alt+Space", "shortcut.global"],
  ["Ctrl+Tab", "shortcut.mode"],
  ["↑ / ↓", "shortcut.arrows"],
  ["Alt+↑ / Alt+↓", "shortcut.direct"],
  ["Alt+Shift+↑ / Alt+Shift+↓", "shortcut.reorder"],
  ["PgUp / PgDn", "shortcut.page"],
  ["→ / ←", "shortcut.level"],
  ["Enter", "shortcut.enter"],
  ["Delete", "shortcut.delete"],
  ["Ctrl+Enter", "shortcut.create"],
  ["Ctrl+D", "shortcut.pin"],
  ["Ctrl+E", "shortcut.edit"],
  ["Ctrl+F", "shortcut.find"],
  ["Esc", "shortcut.escape"],
];
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
  <div class="settings">
    <h2>{t(`preferences.${section}`)}</h2>
    {#if section === "general"}
      <div class="setting">
        <h3>{t("preferences.folder")}</h3>
        <p class="path">{folder?.path ?? t("notes.chooseHint")}</p>
        {#if folder && !folder.available}<p role="alert">{t("notes.unavailable")}</p>{/if}
        <Button disabled={busy} onclick={() => action(async () => { error = (await choose()) ?? ""; await refreshFolder(); })}>{t("notes.choose")}</Button>
      </div>
      <div class="setting"><h3>{t("order.label")}</h3><NoteOrder preferences /><p>{t("order.search")}</p></div>
      <fieldset disabled={busy}>
        <legend>{t("preferences.side")}</legend>
        {#each ["left", "right"] as value}
          <label><input type="radio" name="side" checked={side === value} onchange={() => action(() => panelSetSide(value as PanelFocus["side"]))} />{t(`preferences.${value}` as MessageKey)}</label>
        {/each}
      </fieldset>
    {:else if section === "appearance"}
      <div class="setting">
        <h3>{t("preferences.width")}</h3>
        <p>{t("preferences.widthValue", { width })}</p>
        <p>{t("preferences.widthHint")}</p>
        <Button disabled={busy} onclick={() => action(() => panelResize(null, true))}>{t("preferences.widthReset")}</Button>
      </div>
    {:else if section === "browser"}
      <div class="setting"><h3>{t("preferences.engine")}</h3><p>{t("preferences.engineName")}</p></div>
    {:else}
      <h3>{t("preferences.panelKeys")}</h3>
      <dl>{#each panelShortcuts as [key, description]}<div><dt>{key}</dt><dd>{t(description)}</dd></div>{/each}</dl>
      <h3>{t("preferences.editorKeys")}</h3>
      <dl>
        {#each formattingShortcuts as shortcut}<div><dt>{shortcut.key?.replaceAll("-", "+").replace(/\b[a-z]\b/g, (key) => key.toUpperCase())}</dt><dd>{t(shortcut.description, { level: shortcut.level })}</dd></div>{/each}
        <div><dt>Enter / Backspace</dt><dd>{t("shortcut.continue")}</dd></div>
        <div><dt>Tab / Shift+Tab</dt><dd>{t("shortcut.indent")}</dd></div>
      </dl>
    {/if}
  </div>
</section>

<style>
section { flex: 1; min-height: 0; overflow: auto; padding: var(--space-4) var(--panel-inset-right) var(--space-4) var(--panel-inset-left); }
h1 { font-size: var(--font-size-xl); margin: 0 0 var(--space-3); font-weight: 600; }
nav { display: flex; gap: var(--space-1); padding-bottom: var(--space-3); border-bottom: 1px solid var(--color-line); }
nav button { font: inherit; color: var(--color-muted); background: transparent; border: 0; border-radius: var(--radius-sm); padding: var(--space-1) var(--space-2); }
nav button:hover, nav button.active { background: var(--color-surface); color: var(--color-fg); }
nav button.active { font-weight: 600; }
h2 { font-size: var(--font-size-lg); font-weight: 600; margin: var(--space-4) 0; }
h3, legend { font-size: var(--font-size-md); font-weight: 600; margin: 0 0 var(--space-2); }
p { margin: 0 0 var(--space-3); color: var(--color-muted); }
p[role="alert"] { color: var(--color-danger); }
.path { overflow-wrap: anywhere; }
.setting { padding-bottom: var(--space-4); }
fieldset { border: 0; padding: 0; margin: var(--space-4) 0; }
label { display: inline-flex; align-items: center; gap: var(--space-1); margin-right: var(--space-4); }
input { accent-color: var(--color-accent); }
dl { margin: 0 0 var(--space-4); font-size: var(--font-size-sm); }
dl div { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-3); padding: var(--space-2) 0; border-bottom: 1px solid var(--color-line); }
dt { overflow-wrap: anywhere; }
dd { margin: 0; color: var(--color-muted); }
</style>
