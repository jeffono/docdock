<script lang="ts">
import type { Snippet } from "svelte";
import type { HTMLButtonAttributes } from "svelte/elements";

let {
  children,
  variant = "primary",
  ...rest
}: HTMLButtonAttributes & { children: Snippet; variant?: "primary" | "neutral" } = $props();
</script>

<button class:neutral={variant === "neutral"} {...rest}>{@render children()}</button>

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

  button.neutral {
    background: var(--color-surface);
    color: var(--color-muted);
    font-weight: 400;
  }

  button.neutral:hover {
    color: var(--color-fg);
    opacity: 1;
  }
</style>
