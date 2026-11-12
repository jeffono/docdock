<script lang="ts">
import type { Snippet } from "svelte";
import Icon, { type IconName } from "./Icon.svelte";

/**
 * A tinted notice in the panel column: an icon, a message and its actions on one surface.
 * `attention`: something just happened that its first action undoes; the notice slides in and that
 * action pulses twice, or, with reduced motion, stays highlighted.
 */
let {
  icon,
  children,
  actions,
  attention = false,
}: { icon: IconName; children: Snippet; actions?: Snippet; attention?: boolean } = $props();
</script>

<div class="notice" class:attention role="status">
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

  .attention {
    animation: enter calc(var(--duration) * 2) ease-out;
  }

  .attention .actions > :global(button:first-child) {
    outline: 2px solid transparent;
    outline-offset: 1px;
    animation: pulse calc(var(--duration) * 5) ease-in-out calc(var(--duration) * 2) 2;
  }

  @keyframes enter {
    from {
      opacity: 0;
      transform: translateY(calc(var(--space-2) * -1));
    }
  }

  @keyframes pulse {
    50% {
      outline-color: var(--color-accent);
      background: var(--color-selected);
      color: var(--color-accent);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .attention,
    .attention .actions > :global(button:first-child) {
      animation: none;
    }

    .attention .actions > :global(button:first-child) {
      background: var(--color-selected);
      color: var(--color-accent);
    }
  }
</style>
