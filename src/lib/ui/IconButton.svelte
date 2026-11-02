<script lang="ts">
import type { ComponentProps } from "svelte";
import Icon from "./Icon.svelte";

let {
  icon,
  label,
  pressed,
  onclick,
}: {
  icon: ComponentProps<typeof Icon>["name"];
  label: string;
  /** Só para botões que ligam e desligam. */
  pressed?: boolean;
  onclick: () => void;
} = $props();
</script>

<button aria-label={label} title={label} aria-pressed={pressed} {onclick}>
  <Icon name={icon} />
</button>

<style>
  button {
    display: grid;
    flex: none;
    place-items: center;
    width: var(--size-control);
    height: var(--size-control);
    padding: 0;
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-muted);
    transition:
      background-color var(--duration),
      color var(--duration);
  }

  button:hover {
    background: var(--color-surface);
    color: var(--color-fg);
  }

  button[aria-pressed="true"] {
    border-color: var(--color-accent);
    background: var(--color-selected);
    color: var(--color-accent);
  }
</style>
