<script lang="ts">
import { tweak } from "./state.svelte";
import { colors, contrast, tokens } from "./tokens";

const pairs = [
  ["fg", "bg"],
  ["fg", "surface"],
  ["muted", "bg"],
  ["muted", "surface"],
  ["accent", "bg"],
  ["fg", "selected"],
  ["accent-fg", "accent"],
  ["danger", "bg"],
  ["accent-fg", "danger"],
  ["warning", "warning-bg"],
  ...tokens
    .filter(({ name }) => name.startsWith("--color-code-"))
    .map(({ name }) => [name.slice(8), "surface"]),
];
const color = (name: string, i: number) => colors(tweak.tokens[`--color-${name}`])?.[i] ?? "";
</script>

<details><summary>WCAG contrast · light / dark</summary>
  <p>AA normal text ≥ 4.5:1; large text ≥ 3:1.</p>
  {#each pairs as [fg, bg]}
    <div class="token"><span>{fg} / {bg}</span>
      {#each [0, 1] as i}<small>{i === 0 ? "Light" : "Dark"}: {contrast(color(fg, i), color(bg, i), color("bg", i))}</small>{/each}
    </div>
  {/each}
</details>
