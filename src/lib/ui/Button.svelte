<script lang="ts">
import type { Snippet } from "svelte";
import type { HTMLButtonAttributes } from "svelte/elements";

let { children, ...rest }: HTMLButtonAttributes & { children: Snippet } = $props();
</script>

<button {...rest}>{@render children()}</button>

<style>
  button {
    flex: none;
    height: var(--size-control);
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--color-accent);
    color: var(--color-accent-fg);
    font: inherit;
    font-size: var(--font-size-sm);
    font-weight: 600;
    transition: opacity var(--duration);
  }

  button:hover {
    opacity: 0.88;
  }
</style>
