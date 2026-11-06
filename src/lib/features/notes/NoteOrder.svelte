<script lang="ts">
import { onMount } from "svelte";
import { errorText, t } from "../../i18n";
import { type NoteOrder, noteOrders } from "../../ipc";
import {
  groupByDate,
  loadNoteOrder,
  noteOrder,
  noteOrderBusy,
  setGroupByDate,
  setNoteOrder,
} from "./order-store";

let { preferences = false }: { preferences?: boolean } = $props();
const byName = $derived($noteOrder.startsWith("name"));
let error = $state("");
onMount(() => {
  void loadNoteOrder().catch((e) => {
    error = errorText(e);
  });
});
async function change(event: Event) {
  const input = event.currentTarget as HTMLSelectElement;
  error = "";
  try {
    await setNoteOrder(input.value as NoteOrder);
  } catch (e) {
    error = errorText(e);
    input.value = $noteOrder;
  }
}
async function group(event: Event) {
  const input = event.currentTarget as HTMLInputElement;
  error = "";
  try {
    await setGroupByDate(input.checked);
  } catch (e) {
    error = errorText(e);
  } finally {
    input.checked = $groupByDate;
  }
}
</script>
<div class="note-order" class:preferences>
<select title={t("order.search")} aria-label={t("order.label")} value={$noteOrder} disabled={$noteOrderBusy} onchange={change}>
  {#each noteOrders as order}<option value={order}>{t(`order.${order}`)}</option>{/each}
</select>
<label title={t(byName ? "group.hint" : "group.scope")}>
  <input type="checkbox" checked={$groupByDate} disabled={$noteOrderBusy || byName} onchange={group} />
  {t("group.label")}
</label>
{#if preferences && byName}<p>{t("group.hint")}</p>{/if}
{#if error}<span role="alert">{error}</span>{/if}
</div>
<style>
.note-order { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-1) var(--space-2); min-width: 0; }
.preferences { align-items: flex-start; flex-direction: column; margin-bottom: var(--space-2); }
label { display: inline-flex; align-items: center; gap: var(--space-1); color: var(--color-muted); font-size: var(--font-size-sm); white-space: nowrap; }
input { accent-color: var(--color-accent); margin: 0; }
label:has(:disabled) { opacity: 0.4; }
p { color: var(--color-muted); font-size: var(--font-size-sm); margin: 0; }
select { min-width: 0; max-width: 100%; min-height: var(--size-control); border: 1px solid var(--color-line); border-radius: var(--radius-sm); background: var(--color-surface); color: var(--color-muted); font: inherit; font-size: var(--font-size-sm); padding: var(--space-1); }
span { color: var(--color-danger); font-size: var(--font-size-sm); }
</style>
