<script lang="ts">
import type { ComponentProps } from "svelte";
import Icon from "./Icon.svelte";

let {
  icon,
  label,
  pressed,
  expanded,
  quiet = false,
  accent = false,
  onclick,
  tabindex,
  disabled = false,
}: {
  icon: ComponentProps<typeof Icon>["name"];
  label: string;
  /** Only for toggle buttons. */
  pressed?: boolean;
  /** Only for buttons that show or hide extra controls. */
  expanded?: boolean;
  /** Repeated actions: neutral, borderless, with state conveyed by the icon. */
  quiet?: boolean;
  /** Accent icon without a pressed surface, for the active editor. */
  accent?: boolean;
  onclick: () => void;
  tabindex?: number;
  disabled?: boolean;
} = $props();
</script>

<button class:quiet class:accent aria-label={label} title={label} aria-pressed={pressed} aria-expanded={expanded} {tabindex} {disabled} {onclick}>
  <Icon name={icon} filled={["bookmark", "pin", "clickThrough", "pictureInPicture"].includes(icon) && pressed === true} />
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

  button:hover:not(:disabled) {
    background: var(--color-surface);
    color: var(--color-fg);
  }

  button:not(.quiet)[aria-pressed="true"] {
    border-color: var(--color-accent);
    background: var(--color-selected);
    color: var(--color-accent);
  }

  button.accent:not(:hover) {
    color: var(--color-accent);
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  button.quiet:hover:not(:disabled) {
    background: transparent;
  }

  button.quiet {
    border: 0;
  }
</style>
