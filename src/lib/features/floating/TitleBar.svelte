<script lang="ts">
import { t } from "../../i18n";
import { floatDrag, floatEdit } from "../../ipc";
import IconButton from "../../ui/IconButton.svelte";
import FloatBar from "./FloatBar.svelte";

let {
  name,
  shown,
  opacity,
  clickThrough,
  locale,
  collapsed,
  gone,
  onopacity,
  opacityOpen,
  ontoggleopacity,
  ontoggle,
}: {
  name: string;
  shown: boolean;
  opacity: number;
  clickThrough: boolean;
  locale: string;
  collapsed: boolean;
  gone: boolean;
  onopacity: (value: number) => void;
  opacityOpen: boolean;
  ontoggleopacity: () => void;
  ontoggle: () => void;
} = $props();

const labels = $derived.by(() => {
  void locale;
  return {
    edit: t("postIt.edit"),
    toggle: t(collapsed ? "postIt.expand" : "postIt.collapse"),
  };
});

// The native move loop can take the pointer release away from the WebView. Match two nearby
// presses instead of waiting for a DOM double-click, which depends on both releases.
const DOUBLE_CLICK_TIME = 500;
const DOUBLE_CLICK_DISTANCE = 4;
let lastPress: PointerEvent | undefined;
function pointerdown(event: PointerEvent) {
  if (
    event.button !== 0 ||
    event.ctrlKey ||
    (event.target as Element).closest("button, .opacity-control")
  ) {
    lastPress = undefined;
    return;
  }
  event.preventDefault();
  const previous = lastPress;
  if (
    previous &&
    event.timeStamp - previous.timeStamp <= DOUBLE_CLICK_TIME &&
    Math.hypot(event.screenX - previous.screenX, event.screenY - previous.screenY) <=
      DOUBLE_CLICK_DISTANCE &&
    Math.hypot(event.clientX - previous.clientX, event.clientY - previous.clientY) <=
      DOUBLE_CLICK_DISTANCE
  ) {
    lastPress = undefined;
    ontoggle();
    return;
  }
  lastPress = event;
  void floatDrag().catch(() => {});
}
</script>

{#snippet actions()}
  {#if !gone}
    <IconButton icon="pencil" quiet label={labels.edit} tabindex={-1} onclick={() => void floatEdit().catch(() => {})} />
  {/if}
  <IconButton
    icon={collapsed ? "chevronRight" : "chevronDown"}
    quiet
    label={labels.toggle}
    tabindex={-1}
    onclick={ontoggle}
  />
{/snippet}

<div class="title-bar" class:shown role="group" onpointerdown={pointerdown}>
  <span class="title" title={name}>{name}</span>
  <FloatBar inline {shown} {opacity} {clickThrough} {locale} {onopacity} {actions} {opacityOpen} {ontoggleopacity} />
</div>

<style>
  .title-bar {
    position: absolute;
    inset: var(--pad) var(--pad) auto;
    height: var(--title-height);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--space-2);
    user-select: none;
  }

  .title {
    min-width: 0;
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    opacity: var(--sheet-opacity);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* When the controls are hidden the title can use their space. */
  .title-bar:not(.shown) {
    overflow: clip;
  }

  .title-bar:not(.shown) .title {
    flex: 0 0 100%;
  }

  @media (max-width: 10rem) {
    .title-bar {
      padding-inline: calc(var(--space-1) / 2);
    }
  }
</style>
