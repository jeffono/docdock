<script lang="ts">
import { t } from "../../i18n";
import Icon, { type IconName } from "../../ui/Icon.svelte";
import { type Counts, formatTimeRange, readingRates, speakingRates } from "./count";

let {
  total,
  selected = null,
  full = true,
}: {
  total: Counts;
  selected?: Counts | null;
  /** Everything; otherwise characters, reading time and images, and everything while hovered. */
  full?: boolean;
} = $props();
const number = new Intl.NumberFormat(document.documentElement.lang || "en");
let hovered = $state(false);

type Unit = "Words" | "Chars" | "Images";
/** `count` of a unit, or "selected of total" while a selection counts. */
function amount(unit: Unit, quantity = (counts: Counts) => counts[key(unit)]) {
  const all = quantity(total);
  const part = selected ? quantity(selected) : undefined;
  return part === undefined
    ? number.format(all)
    : t("editor.countOf", { selected: number.format(part), total: number.format(all) });
}
const key = (unit: Unit) => unit.toLowerCase() as keyof Counts;
const counted = (unit: Unit) =>
  t(`editor.count${unit}.${total[key(unit)] === 1 ? "one" : "other"}`, { count: amount(unit) });

type Item = { key: string; extra: boolean; text: string; icon?: IconName; label?: string };
const items = $derived.by(() => {
  const words = (selected ?? total).words;
  const list: Item[] = [];
  // A note of images alone has no prose to count.
  if (total.chars > 0 || total.images === 0) {
    list.push({ key: "words", extra: true, text: counted("Words") });
    list.push({ key: "chars", extra: false, text: counted("Chars") });
  }
  if (words > 0) {
    const reading = formatTimeRange(words, ...readingRates);
    const speaking = formatTimeRange(words, ...speakingRates);
    list.push({
      key: "reading",
      extra: false,
      icon: "reader",
      label: t("editor.readingTime"),
      text: reading,
    });
    list.push({
      key: "speaking",
      extra: true,
      icon: "mic",
      label: t("editor.speakingTime"),
      text: speaking,
    });
  }
  if (total.images > 0)
    list.push({
      key: "images",
      extra: false,
      icon: "image",
      label: counted("Images"),
      text: amount("Images"),
    });
  return list;
});
const shown = $derived(full || hovered ? items : items.filter((item) => !item.extra));
</script>

<p
  class="count"
  aria-live="off"
  onpointerenter={() => { hovered = true; }}
  onpointerleave={() => { hovered = false; }}
>
  {#each shown as item, index (item.key)}
    <span class="item">
      {#if index > 0}<span aria-hidden="true">·</span>{/if}
      {#if item.icon}
        <span class="figure" role="group" aria-label={item.label} title={item.label}><Icon name={item.icon} />{item.text}</span>
      {:else}
        {item.text}
      {/if}
    </span>
  {/each}
</p>

<style>
  .count {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    column-gap: var(--space-1);
    margin: 0;
    padding: 0 var(--space-3) var(--space-1);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    text-align: right;
  }

  .item, .figure {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    white-space: nowrap;
  }

  .figure {
    --size-icon: var(--font-size-sm);
  }
</style>
