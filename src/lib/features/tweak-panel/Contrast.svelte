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

<h2>WCAG contrast</h2>
<p>Normal text: AA >= 4.5; AAA >= 7. Large text: AA >= 3; AAA >= 4.5.</p>
<table>
<thead><tr><th>Pair / theme</th><th>Ratio</th><th>AA</th><th>AAA</th></tr></thead>
<tbody>{#each pairs as [fg, bg]}{#each [0, 1] as i}
{@const result = contrast(color(fg, i), color(bg, i), color("bg", i))}
<tr><th scope="row">{fg} / {bg}<small>{i === 0 ? "Light" : "Dark"}</small></th><td>{result?.ratio ?? "Unavailable"}</td><td>{result ? result.aa ? "Pass" : "Fail" : "Unavailable"}</td><td>{result ? result.aaa ? "Pass" : "Fail" : "Unavailable"}</td></tr>
{/each}{/each}</tbody></table>
<style>
h2 { font-size: var(--font-size-md); margin: 0; }
p { color: var(--color-muted); }
table { width: 100%; border-collapse: collapse; font-size: var(--font-size-sm); }
th, td { text-align: left; padding: var(--space-1); border-bottom: 1px solid var(--color-line); overflow-wrap: anywhere; }
th { font-weight: normal; } small { display: block; }
thead { position: sticky; top: 0; background: var(--color-bg); }
</style>
