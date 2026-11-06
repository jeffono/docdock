<script lang="ts">
import type { Snippet } from "./search";

let { snippet }: { snippet: Snippet } = $props();

// Text and highlights are separate nodes, so note text never becomes markup.
const parts = $derived.by(() => {
  const out: { text: string; match: boolean }[] = [];
  let at = 0;
  for (const { from, to } of snippet.ranges) {
    if (from > at) out.push({ text: snippet.text.slice(at, from), match: false });
    out.push({ text: snippet.text.slice(from, to), match: true });
    at = to;
  }
  if (at < snippet.text.length) out.push({ text: snippet.text.slice(at), match: false });
  return out;
});
</script>

<span class="snippet">{#if snippet.before}…{/if}{#each parts as part}{#if part.match}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}{#if snippet.after}…{/if}</span>

<style>
  .snippet {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  mark {
    background: var(--color-selected);
    color: var(--color-accent);
  }
</style>
