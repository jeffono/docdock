<script lang="ts">
import { onMount } from "svelte";
import { errorText, t } from "../../i18n";
import { type NoteOrder, noteOrders } from "../../ipc";
import { loadNoteOrder, noteOrder, noteOrderBusy, setNoteOrder } from "./order-store";

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
</script>
<select title={t("order.search")} aria-label={t("order.label")} value={$noteOrder} disabled={$noteOrderBusy} onchange={change}>
  {#each noteOrders as order}<option value={order}>{t(`order.${order}`)}</option>{/each}
</select>
{#if error}<span role="alert">{error}</span>{/if}
<style>
select { min-width: 0; max-width: 100%; min-height: var(--size-control); border: 1px solid var(--color-line); border-radius: var(--radius-sm); background: var(--color-surface); color: var(--color-muted); font: inherit; font-size: var(--font-size-sm); padding: var(--space-1); }
span { color: var(--color-danger); font-size: var(--font-size-sm); }
</style>
