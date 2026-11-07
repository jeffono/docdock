<script lang="ts">
import { onMount } from "svelte";
import { errorText, t } from "../../i18n";
import { type Grouping, type NoteOrder, noteOrders } from "../../ipc";
import Menu from "../../ui/Menu.svelte";
import {
  grouping,
  loadNoteOrder,
  noteOrder,
  noteOrderBusy,
  setGrouping,
  setNoteOrder,
} from "./order-store";

let { preferences = false }: { preferences?: boolean } = $props();
const id = $props.id();
const byName = $derived($noteOrder.startsWith("name"));
let error = $state("");
let open = $state(false);
let trigger: HTMLButtonElement;
let menu: Menu;
onMount(() => {
  void loadNoteOrder().catch((e) => {
    error = errorText(e);
  });
});
function show(last = false) {
  if (!$noteOrderBusy) menu.show({ last, from: trigger });
}
/** Each grouping item toggles itself; checking one clears the other. */
async function change(order?: NoteOrder, group?: Grouping) {
  error = "";
  try {
    if (order) await setNoteOrder(order);
    else if (group) await setGrouping($grouping === group ? "none" : group);
  } catch (e) {
    error = errorText(e);
  }
}
function triggerKey(event: KeyboardEvent) {
  if (!["Enter", " ", "ArrowDown", "ArrowUp"].includes(event.key)) return;
  event.preventDefault();
  event.stopPropagation();
  show(event.key === "ArrowUp");
}
</script>

<div class="note-order" class:preferences>
  <button bind:this={trigger} class="trigger" aria-label={`${t("order.label")}: ${t(`order.${$noteOrder}`)}`} aria-haspopup="menu" aria-expanded={open} aria-controls={id} aria-disabled={$noteOrderBusy} title={t("order.search")} onclick={() => open ? menu.close() : show()} onkeydown={triggerKey}>
    {t(`order.${$noteOrder}`)}
  </button>
  <Menu bind:this={menu} {id} class="order-menu" label={t("order.label")} ontoggle={(value) => { open = value; }}>
    <div role="group" aria-label={t("order.label")}>
      {#each noteOrders as order}
        <button role="menuitemradio" aria-checked={$noteOrder === order} tabindex="-1" onclick={() => change(order)}>
          <span class="check" aria-hidden="true">{$noteOrder === order ? "✓" : ""}</span>{t(`order.${order}`)}
        </button>
      {/each}
    </div>
    <hr />
    <button role="menuitemcheckbox" aria-checked={$grouping === "date"} disabled={byName} aria-describedby={byName ? `${id}-hint` : undefined} title={t(byName ? "group.hint" : "group.scope")} tabindex="-1" onclick={() => change(undefined, "date")}>
      <span class="check" aria-hidden="true">{$grouping === "date" ? "✓" : ""}</span>{t("group.label")}
    </button>
    {#if byName}<p id={`${id}-hint`}>{t("group.hint")}</p>{/if}
    <button role="menuitemcheckbox" aria-checked={$grouping === "folder"} title={t("group.folderScope")} tabindex="-1" onclick={() => change(undefined, "folder")}>
      <span class="check" aria-hidden="true">{$grouping === "folder" ? "✓" : ""}</span>{t("group.folder")}
    </button>
  </Menu>
  {#if error}<span class="error" role="alert">{error}</span>{/if}
</div>

<style>
.note-order { min-width: 0; margin-left: auto; }
.preferences { margin-bottom: var(--space-2); }
.trigger { anchor-name: --note-order; border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--color-muted); font: inherit; font-size: var(--font-size-sm); min-height: var(--size-control); padding: var(--space-1) var(--space-2); text-align: left; }
.trigger:hover { background: var(--color-surface); color: var(--color-fg); }
.note-order :global(.order-menu) { position-anchor: --note-order; position-area: bottom span-right; position-try-fallbacks: flip-block, flip-inline, flip-block flip-inline; }
.error { color: var(--color-danger); font-size: var(--font-size-sm); }
</style>
