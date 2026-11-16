<script lang="ts">
import FontSelect from "./FontSelect.svelte";
import { editToken, tweak } from "./state.svelte";
import { colors, defaults, tokens } from "./tokens";

const groups = [
  ["Colors", "--color-"],
  ["Fonts", "--font-"],
  ["Spacing", "--space-"],
  ["Radius", "--radius-"],
  ["Control sizes", "--size-"],
  ["Line height", "--line-"],
  ["Tracking", "--tracking-"],
  ["Animation", "--duration"],
];

function editColor(token: string, index: number, value: string) {
  const pair = colors(tweak.tokens[token]);
  if (!pair) return;
  pair[index] = value;
  editToken(token, `light-dark(${pair.join(", ")})`);
}
</script>

{#each groups as [label, prefix]}
  <details><summary>{label}</summary>
    {#each tokens.filter(({ name }) => name.startsWith(prefix)) as token}
      {@const pair = colors(tweak.tokens[token.name])}
      <div class="token">
        <span>{token.name}</span>
        {#if pair}
          <div class="colors">
            {#each pair as color, i}
              <label>{i === 0 ? "Light" : "Dark"}<input aria-label={`${token.name} ${i === 0 ? "light" : "dark"}`} value={color} onchange={(e) => editColor(token.name, i, e.currentTarget.value)} />
              {#if /^#[\da-f]{6}$/i.test(color)}<input type="color" aria-label={`${token.name} ${i === 0 ? "light" : "dark"} picker`} value={color} oninput={(e) => editColor(token.name, i, e.currentTarget.value)} />{/if}</label>
            {/each}
          </div>
        {:else if token.name === "--font-sans" || token.name === "--font-mono"}
          <FontSelect name={token.name} value={tweak.tokens[token.name]} initial={defaults[token.name]} onchange={(value) => editToken(token.name, value)} />
        {:else}
          <input aria-label={token.name} value={tweak.tokens[token.name]} onchange={(e) => editToken(token.name, e.currentTarget.value)} />
        {/if}
      </div>
    {/each}
  </details>
{/each}

<style>
  .colors { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: var(--space-2); }
  input[type="color"] { width: var(--size-control); padding: 0; }
</style>
