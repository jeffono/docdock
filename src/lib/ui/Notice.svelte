<script lang="ts">
import type { Snippet } from "svelte";
import Icon, { type IconName } from "./Icon.svelte";

/** A tinted notice in the panel column: an icon, a message and its actions on one surface. */
let { icon, children, actions }: { icon: IconName; children: Snippet; actions?: Snippet } =
  $props();
</script>

<div class="notice" role="status">
  <span class="icon"><Icon name={icon} /></span>
  <p>{@render children()}</p>
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</div>

<style>
  .notice {
    display: flex;
    flex: none;
    gap: var(--space-2);
    align-items: center;
    margin: 0 var(--panel-inset-right) var(--space-2) var(--panel-inset-left);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-3);
    border: 1px solid var(--color-warning-line);
    border-radius: var(--radius-sm);
    background: var(--color-warning-bg);
    color: var(--color-fg);
    font-size: var(--font-size-sm);
  }

  .icon {
    display: flex;
    flex: none;
    color: var(--color-warning);
  }

  p {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    flex: none;
    gap: var(--space-1);
    align-items: center;
  }
</style>
