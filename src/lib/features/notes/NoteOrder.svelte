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
const id = $props.id();
const byName = $derived($noteOrder.startsWith("name"));
let error = $state("");
let open = $state(false);
let trigger: HTMLButtonElement;
let menu: HTMLDivElement;
onMount(() => {
  void loadNoteOrder().catch((e) => {
    error = errorText(e);
  });
});
function close() {
  menu.hidePopover();
  trigger.focus();
}
function show(last = false) {
  if ($noteOrderBusy) return;
  menu.showPopover();
  const items = menu.querySelectorAll<HTMLButtonElement>("button:not(:disabled)");
  (last
    ? items[items.length - 1]
    : (menu.querySelector<HTMLButtonElement>('[aria-checked="true"]') ?? items[0])
  )?.focus();
}
async function change(order?: NoteOrder) {
  close();
  error = "";
  try {
    if (order) await setNoteOrder(order);
    else await setGroupByDate(!$groupByDate);
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
function menuKey(event: KeyboardEvent) {
  event.stopPropagation();
  const items = [...menu.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
  const at = items.indexOf(document.activeElement as HTMLButtonElement);
  if (event.key === "Escape" || event.key === "Tab") {
    event.preventDefault();
    close();
  } else if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    items[at]?.click();
  } else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
    event.preventDefault();
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? items.length - 1
          : (at + (event.key === "ArrowDown" ? 1 : items.length - 1)) % items.length;
    items[next]?.focus();
  }
}
</script>

<div class="note-order" class:preferences>
  <button bind:this={trigger} class="trigger" aria-label={`${t("order.label")}: ${t(`order.${$noteOrder}`)}`} aria-haspopup="menu" aria-expanded={open} aria-controls={id} aria-disabled={$noteOrderBusy} title={t("order.search")} onclick={() => open ? close() : show()} onkeydown={triggerKey}>
    {t(`order.${$noteOrder}`)}
  </button>
  <div bind:this={menu} {id} class="menu" popover="auto" role="menu" tabindex="-1" aria-label={t("order.label")} onkeydown={menuKey} ontoggle={() => { open = menu.matches(":popover-open"); }}>
    <div role="group" aria-label={t("order.label")}>
      {#each noteOrders as order}
        <button role="menuitemradio" aria-checked={$noteOrder === order} tabindex="-1" onclick={() => change(order)}>
          <span class="check" aria-hidden="true">{$noteOrder === order ? "✓" : ""}</span>{t(`order.${order}`)}
        </button>
      {/each}
    </div>
    <hr />
    <button role="menuitemcheckbox" aria-checked={$groupByDate} disabled={byName} aria-describedby={byName ? `${id}-hint` : undefined} title={t(byName ? "group.hint" : "group.scope")} tabindex="-1" onclick={() => change()}>
      <span class="check" aria-hidden="true">{$groupByDate ? "✓" : ""}</span>{t("group.label")}
    </button>
    {#if byName}<p id={`${id}-hint`}>{t("group.hint")}</p>{/if}
  </div>
  {#if error}<span class="error" role="alert">{error}</span>{/if}
</div>

<style>
.note-order { min-width: 0; margin-left: auto; }
.preferences { margin-bottom: var(--space-2); }
button { border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--color-muted); font: inherit; font-size: var(--font-size-sm); min-height: var(--size-control); padding: var(--space-1) var(--space-2); text-align: left; }
button:hover, .menu button:focus-visible { background: var(--color-bg); color: var(--color-fg); }
.trigger { anchor-name: --note-order; }
.trigger:hover { background: var(--color-surface); }
.menu { position: fixed; position-anchor: --note-order; position-area: bottom span-right; position-try-fallbacks: flip-block, flip-inline, flip-block flip-inline; inset: auto; margin: var(--space-1) 0; padding: var(--space-1); width: max-content; max-width: calc(100dvw - var(--space-4) * 2); max-height: calc(100dvh - var(--space-4) * 2); overflow: auto; box-sizing: border-box; border: 1px solid var(--color-line); border-radius: var(--radius-sm); background: var(--color-surface); color: var(--color-fg); }
.menu button { display: flex; align-items: center; gap: var(--space-2); width: 100%; color: var(--color-fg); }
.menu button:disabled { color: var(--color-muted); cursor: default; }
.check { flex: none; width: var(--size-icon); }
hr { border: 0; border-top: 1px solid var(--color-line); margin: var(--space-1) 0; }
p { color: var(--color-muted); font-size: var(--font-size-sm); margin: var(--space-1) var(--space-2); max-width: calc(var(--size-control) * 8); }
.error { color: var(--color-danger); font-size: var(--font-size-sm); }
</style>
