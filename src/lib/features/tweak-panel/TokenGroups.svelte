<script lang="ts">
import ColorPicker from "./ColorPicker.svelte";
import FontPicker from "./FontPicker.svelte";
import NumberField from "./NumberField.svelte";
import { editToken, tweak } from "./state.svelte";
import { colors, defaults, tokens } from "./tokens";

const groups = [
  ["Text and typography", ["--font-", "--line-", "--tracking-"]],
  ["Colors", ["--color-"]],
  ["Spacing and layout", ["--space-", "--radius-", "--size-"]],
  ["CSS transitions (not native slides)", ["--duration"]],
] as const;
function numeric(value: string) {
  return /^(-?[\d.]+)(rem|em|px|ms|s)?$/.exec(value);
}

function editColor(token: string, index: number, value: string) {
  const pair = colors(tweak.tokens[token]);
  if (!pair) return;
  pair[index] = value;
  editToken(token, `light-dark(${pair.join(", ")})`);
}
</script>

{#each groups as [label, prefixes]}
  <details><summary>{label}</summary>
    {#each tokens.filter(({ name }) => prefixes.some(prefix => name.startsWith(prefix))) as token}
      {@const pair = colors(tweak.tokens[token.name])}
      <div class="token">
        <span>{token.name}</span>
        {#if pair}
          <div class="colors">
            {#each pair as color, i}
              <div><small>{i === 0 ? "Light" : "Dark"}</small><ColorPicker label={`${token.name} ${i === 0 ? "light" : "dark"}`} value={color} onchange={(value) => editColor(token.name, i, value)} /></div>
            {/each}
          </div>
        {:else if token.name === "--font-sans" || token.name === "--font-mono"}
          <FontPicker name={token.name} value={tweak.tokens[token.name]} initial={defaults[token.name]} onchange={(value) => editToken(token.name, value)} />
        {:else}
          {@const number = numeric(tweak.tokens[token.name])}
          {#if number}
            <label>{number[2] || "unitless"}<NumberField label={token.name} value={Number(number[1])} min={token.name.startsWith("--tracking") ? -1 : 0} max={token.name === "--duration" ? (number[2] === "s" ? 2 : 2000) : 100} step={number[2] === "ms" ? 10 : number[2] === "px" ? 1 : 0.01} onchange={(value) => editToken(token.name, `${value}${number[2] ?? ""}`)} /></label>
          {:else}
            <input aria-label={token.name} value={tweak.tokens[token.name]} oninput={(e) => editToken(token.name, e.currentTarget.value)} />
          {/if}
        {/if}
      </div>
    {/each}
  </details>
{/each}

<style>
  .colors { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: var(--space-2); }

</style>
