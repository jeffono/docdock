<script lang="ts">
import { t } from "../../i18n";
import Icon from "../../ui/Icon.svelte";
import { type Counts, formatTimeRange, readingRates, speakingRates } from "./count";

let { total, selected = null }: { total: Counts; selected?: Counts | null } = $props();
const number = new Intl.NumberFormat(document.documentElement.lang || "en");
const words = $derived((selected ?? total).words);
const estimates = $derived([
  { icon: "reader", label: t("editor.readingTime"), time: formatTimeRange(words, ...readingRates) },
  { icon: "mic", label: t("editor.speakingTime"), time: formatTimeRange(words, ...speakingRates) },
] as const);

function countLabel(unit: "Words" | "Chars", total: number, selected: number | undefined) {
  const count =
    selected === undefined
      ? number.format(total)
      : t("editor.countOf", { selected: number.format(selected), total: number.format(total) });
  return t(`editor.count${unit}.${total === 1 ? "one" : "other"}`, { count });
}
</script>

<p class="count" aria-live="off">
  <span>{t("editor.count", {
    words: countLabel("Words", total.words, selected?.words),
    chars: countLabel("Chars", total.chars, selected?.chars),
  })}</span>
  {#if words > 0}
    {#each estimates as { icon, label, time } (icon)}
      <span class="estimate">
        <span aria-hidden="true">·</span>
        <span class="duration" role="group" aria-label={label} title={label}><Icon name={icon} />{time}</span>
      </span>
    {/each}
  {/if}
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

  .estimate, .duration {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    white-space: nowrap;
  }

  .duration {
    --size-icon: var(--font-size-sm);
  }
</style>
