<script lang="ts">
import type { ComponentProps } from "svelte";
import Icon from "./Icon.svelte";

let {
  icon,
  label,
  pressed,
  quiet = false,
  onclick,
}: {
  icon: ComponentProps<typeof Icon>["name"];
  label: string;
  /** Only for toggle buttons. */
  pressed?: boolean;
  /** Repeated actions: neutral, borderless, with state conveyed by the icon. */
  quiet?: boolean;
  onclick: () => void;
} = $props();
</script>

<button class:quiet aria-label={label} title={label} aria-pressed={pressed} {onclick}>
  <Icon name={icon} filled={icon === "bookmark" && pressed === true} />
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

  button:not(.quiet)[aria-pressed="true"] {
    border-color: var(--color-accent);
    background: var(--color-selected);
    color: var(--color-accent);
  }

  button.quiet {
    border: 0;
  }
</style>
