<script lang="ts">
import { t } from "../../i18n";
import { floatDrag } from "../../ipc";
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
  editing,
  status,
  canCollapse,
  onopacity,
  opacityOpen,
  ontoggleopacity,
  ontoggle,
  onedit,
  onpanel,
  onclose,
}: {
  name: string;
  shown: boolean;
  opacity: number;
  clickThrough: boolean;
  locale: string;
  collapsed: boolean;
  gone: boolean;
  /** The note is edited here, or on its way in or out. */
  editing: boolean;
  /** "Saving…" or "Saved" while editing, else empty. */
  status: string;
  /** Only while reading: an edit keeps the post-it expanded. */
  canCollapse: boolean;
  onopacity: (value: number) => void;
  opacityOpen: boolean;
  ontoggleopacity: () => void;
  ontoggle: () => void;
  onedit: () => void;
  onpanel: () => void;
  onclose: () => void;
} = $props();

const labels = $derived.by(() => {
  void locale;
  return {
    edit: t("postIt.edit"),
    panel: t("postIt.openInPanel"),
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
    if (canCollapse) ontoggle();
    return;
  }
  lastPress = event;
  void floatDrag().catch(() => {});
}
</script>

{#snippet actions()}
  {#if !gone}
    {#if !editing}
      <IconButton icon="pencil" quiet label={labels.edit} tabindex={-1} onclick={onedit} />
    {/if}
    <IconButton icon="panelRight" quiet label={labels.panel} tabindex={-1} onclick={onpanel} />
  {/if}
{/snippet}

<div class="title-bar" class:shown class:collapsed role="group" onpointerdown={pointerdown}>
  <div class="collapse-toggle">
    <IconButton
      icon={collapsed ? "chevronRight" : "chevronDown"}
      quiet
      label={labels.toggle}
      expanded={!collapsed}
      disabled={!canCollapse}
      onclick={ontoggle}
    />
  </div>
  <span class="title" title={name}>{name}</span>
  {#if status}<small class="status" role="status">{status}</small>{/if}
  <FloatBar
    inline
    {shown}
    {opacity}
    {clickThrough}
    {locale}
    {onopacity}
    {actions}
    {opacityOpen}
    {ontoggleopacity}
    canClickThrough={!editing}
    {onclose}
  />
</div>

<style>
  .title-bar {
    --chevron-gutter: var(--size-control);
    position: absolute;
    inset: var(--pad) var(--pad) auto;
    height: var(--title-height);
    display: flex;
    align-items: center;
    padding: 0 var(--space-2);
    user-select: none;
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    opacity: var(--sheet-opacity);
    text-overflow: ellipsis;
    white-space: nowrap;
    transform: translateX(calc(-1 * var(--chevron-gutter)));
    transition: transform var(--duration) ease;
  }

  .status {
    flex: none;
    margin: 0 var(--space-1);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
    white-space: nowrap;
  }

  /* Reserve the gutter so a long title keeps its ellipsis as it slides. */
  .collapse-toggle {
    flex: none;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--duration);
  }

  .title-bar:is(.shown, .collapsed, :has(:global(:focus-visible))) .collapse-toggle {
    opacity: 1;
    pointer-events: auto;
  }

  .title-bar:is(.shown, .collapsed, :has(:global(:focus-visible))) .title {
    transform: none;
  }

  @media (max-width: 10rem) {
    .title-bar {
      --chevron-gutter: calc(var(--size-icon) + var(--space-1) / 2);
      padding-inline: calc(var(--space-1) / 2);
    }

    .collapse-toggle :global(button) {
      width: var(--chevron-gutter);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .title,
    .collapse-toggle {
      transition: none;
    }
  }
</style>
