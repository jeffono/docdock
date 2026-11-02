<script lang="ts">
import type { HTMLInputAttributes } from "svelte/elements";

let { value = $bindable(""), ...rest }: HTMLInputAttributes = $props();

function leave(event: KeyboardEvent) {
  if (event.key === "Escape") {
    (event.currentTarget as HTMLElement).blur();
  }
}
</script>

<input bind:value onkeydown={leave} {...rest} />

<style>
  input {
    flex: 1;
    box-sizing: border-box;
    min-width: 0;
    height: var(--size-control);
    padding: 0 var(--space-2);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-fg);
    font: inherit;
    font-size: var(--font-size-sm);
  }

  input::placeholder {
    color: var(--color-muted);
  }
</style>
