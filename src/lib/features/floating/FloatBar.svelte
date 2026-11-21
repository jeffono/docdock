<script lang="ts">
// The pill of controls a floating window shows in its top right corner: what comes `before` (an
// image's zoom), the opacity slider, any `actions`, then letting clicks through and closing. It
// stays opaque while the window fades, and keeps the class `bar`, which the window's wheel reads.
import type { Snippet } from "svelte";
import { t } from "../../i18n";
import { floatClose, floatSetClickThrough } from "../../ipc";
import Icon from "../../ui/Icon.svelte";
import IconButton from "../../ui/IconButton.svelte";
import { MIN_OPACITY, OPACITY_STEP } from "./opacity";

let {
  shown,
  opacity,
  clickThrough,
  locale,
  onopacity,
  before,
  actions,
}: {
  shown: boolean;
  opacity: number;
  clickThrough: boolean;
  /** Changes with the language, so the labels follow. */
  locale: string;
  /** The user moved the slider: the window shows it at once and sends it (`opacitySender`). */
  onopacity: (value: number) => void;
  before?: Snippet;
  actions?: Snippet;
} = $props();

let range: HTMLInputElement;
const labels = $derived.by(() => {
  void locale;
  return {
    opacity: t("float.opacity"),
    clickThrough: t("float.clickThrough"),
    close: t("float.close"),
  };
});
// The slider's filled part, through the CSSOM: the CSP forbids style attributes.
$effect(() => {
  const filled = ((opacity - MIN_OPACITY) / (1 - MIN_OPACITY)) * 100;
  range.style.setProperty("--fill", `${filled}%`);
});
</script>


<div class="bar" class:shown>
  {#if before}{@render before()}{/if}
  <span class="opacity" class:apart={!!before}>
    <Icon name="blend" />
    <input
      bind:this={range}
      type="range"
      min={MIN_OPACITY * 100}
      max="100"
      step={OPACITY_STEP * 100}
      value={Math.round(opacity * 100)}
      aria-label={labels.opacity}
      title={`${labels.opacity}: ${Math.round(opacity * 100)}%`}
      aria-valuetext={`${Math.round(opacity * 100)}%`}
      tabindex="-1"
      oninput={(event) => onopacity(Number(event.currentTarget.value) / 100)}
    />
  </span>
  {#if actions}{@render actions()}{/if}
  <IconButton
    icon="clickThrough"
    quiet
    label={labels.clickThrough}
    pressed={clickThrough}
    tabindex={-1}
    onclick={() => void floatSetClickThrough(!clickThrough).catch(() => {})}
  />
  <IconButton icon="x" quiet label={labels.close} tabindex={-1} onclick={() => void floatClose().catch(() => {})} />
</div>

<style>
  /* `--pad` is the window's transparent gutter, set by the window. */
  .bar {
    position: absolute;
    top: calc(var(--pad) + var(--space-2));
    right: calc(var(--pad) + var(--space-2));
    display: flex;
    align-items: center;
    padding: 0 calc(var(--space-1) / 2) 0 var(--space-1);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-full);
    background: var(--color-surface);
    visibility: hidden;
    opacity: 0;
    transition:
      opacity var(--duration),
      visibility var(--duration);
  }

  .bar.shown {
    visibility: visible;
    opacity: 1;
  }

  /* The opacity's own group, an icon and its slider, set apart from what comes before by a line
     so the slider never reads as its. */
  .opacity {
    display: flex;
    align-items: center;
    color: var(--color-muted);
  }

  .opacity.apart {
    margin-left: var(--space-1);
    padding-left: var(--space-2);
    border-left: 1px solid var(--color-line);
  }

  .opacity :global(svg) {
    width: calc(0.875 * var(--size-icon));
    height: calc(0.875 * var(--size-icon));
  }

  /* A short, thin monochrome track, solid up to the thumb, and a small round thumb: no accent,
     nothing to compete with the content. `--fill` comes from the script. */
  input[type="range"] {
    width: calc(1.5 * var(--size-control));
    height: var(--size-control);
    margin: 0 var(--space-1);
    appearance: none;
    background: transparent;
  }

  input[type="range"]::-webkit-slider-runnable-track {
    height: calc(var(--space-1) / 2);
    border-radius: var(--radius-full);
    background: linear-gradient(
      to right,
      var(--color-fg) var(--fill, 100%),
      var(--color-line) var(--fill, 100%)
    );
  }

  input[type="range"]::-webkit-slider-thumb {
    width: calc(2 * var(--space-1));
    height: calc(2 * var(--space-1));
    margin-top: calc(-0.75 * var(--space-1));
    border: 0;
    border-radius: var(--radius-full);
    appearance: none;
    background: var(--color-fg);
  }

  /* A narrow window keeps room for its actions, toggling and closing. */
  @media (max-width: 12rem) {
    .opacity {
      display: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .bar {
      transition: none;
    }
  }
</style>
