<script lang="ts">
import { onMount } from "svelte";
import Button from "../../ui/Button.svelte";
import { colors, contrast, defaults, exportDiff, tokens, type Values, valid } from "./tokens";

let { hold }: { hold: (open: boolean) => Promise<void> } = $props();
let open = $state(false);
let busy = $state(false);
let error = $state("");
let values = $state<Values>({ ...defaults });
let presets = $state<Record<string, Values>>({});
let name = $state("");
let selected = $state("");
let diff = $state("");
const key = "docdock-tweak-presets-v1";
const fonts = [
  '"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif',
  '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif',
  '"Segoe UI", system-ui, sans-serif',
  "Arial, sans-serif",
  "Verdana, sans-serif",
  "Georgia, serif",
  '"Cascadia Mono", Consolas, monospace',
  "Consolas, monospace",
];
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

onMount(() => {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(key) ?? "{}");
    if (stored && typeof stored === "object" && !Array.isArray(stored)) {
      for (const [label, preset] of Object.entries(stored)) {
        if (preset && typeof preset === "object" && !Array.isArray(preset)) {
          presets[label] = <Values>(
            Object.fromEntries(
              Object.entries(preset as Record<string, unknown>).filter(
                ([name, value]) =>
                  name in defaults && typeof value === "string" && valid(name, value as string),
              ),
            )
          );
        }
      }
    }
  } catch {
    error = "Could not read saved presets.";
  }
});

async function toggle() {
  if (busy) return;
  busy = true;
  try {
    await hold(!open);
    open = !open;
    error = "";
  } catch (e) {
    error = String(e);
  } finally {
    busy = false;
  }
}
function edit(token: string, value: string) {
  if (!valid(token, value)) {
    error = `Invalid CSS value for ${token}`;
    return;
  }
  document.documentElement.style.setProperty(token, value);
  values[token] = value;
  diff = "";
  error = "";
}
function editColor(token: string, index: number, value: string) {
  const pair = colors(values[token]);
  if (!pair) return;
  pair[index] = value;
  edit(token, `light-dark(${pair.join(", ")})`);
}
function apply(preset: Values) {
  for (const { name } of tokens) edit(name, preset[name] ?? defaults[name]);
}
function save() {
  if (!name.trim()) return;
  // Only edited tokens, so a preset follows later changes to the source.
  const edited = Object.fromEntries(
    Object.entries(values).filter(([token, value]) => value !== defaults[token]),
  );
  const next = { ...presets, [name.trim()]: edited };
  try {
    localStorage.setItem(key, JSON.stringify(next));
    presets = next;
    selected = name.trim();
    error = "";
  } catch {
    error = "Could not save presets. Export the diff to keep these changes.";
  }
}
</script>

<svelte:window onkeydown={(event) => { if (open && event.key === "Escape" && !event.defaultPrevented) { event.preventDefault(); void toggle(); } }} />
<div class="launcher"><Button variant="neutral" onclick={toggle} disabled={busy}>Tweak</Button></div>
{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <section class="tweak-panel" aria-label="Development token tweaks" onkeydown={(event) => { if (event.key !== "Escape") event.stopPropagation(); }}>
    <div class="toolbar"><strong>Tweak panel · DEV</strong><Button variant="neutral" onclick={toggle} disabled={busy}>Close</Button></div>
    <p>Live preview · changes stay until reload. Closing restores the usual pin rule.</p>
    <div class="toolbar">
      <select aria-label="Named preset" bind:value={selected} onchange={() => { if (presets[selected]) apply(presets[selected]); }}>
        <option value="">Choose preset</option>
        {#each Object.keys(presets) as preset}<option value={preset}>{preset}</option>{/each}
      </select>
      <Button variant="neutral" onclick={() => { apply(defaults); selected = ""; }}>Reset</Button>
    </div>
    <div class="toolbar"><input aria-label="Preset name" placeholder="Preset name (A / B)" bind:value={name} /><Button variant="neutral" onclick={save}>Save</Button></div>
    {#if error}<p role="alert">{error}</p>{/if}
    {#each groups as [label, prefix]}
      <details><summary>{label}</summary>
        {#each tokens.filter(({ name }) => name.startsWith(prefix)) as token}
          {@const pair = colors(values[token.name])}
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
              <select aria-label={token.name} value={values[token.name]} onchange={(e) => edit(token.name, e.currentTarget.value)}>
                {#each [...new Set([defaults[token.name], ...fonts])] as font}<option value={font}>{font.split(",")[0]}</option>{/each}
              </select>
            {:else}
              <input aria-label={token.name} value={values[token.name]} onchange={(e) => edit(token.name, e.currentTarget.value)} />
            {/if}
          </div>
        {/each}
      </details>
    {/each}
    <details><summary>WCAG contrast · light / dark</summary>
      <p>AA normal text ≥ 4.5:1; large text ≥ 3:1.</p>
      {#each pairs as [fg, bg]}
        <div class="token"><span>{fg} / {bg}</span>
          {#each [0, 1] as i}<small>{i === 0 ? "Light" : "Dark"}: {contrast(colors(values[`--color-${fg}`])![i], colors(values[`--color-${bg}`])![i], colors(values["--color-bg"])![i])}</small>{/each}
        </div>
      {/each}
    </details>
    <Button variant="neutral" onclick={() => { diff = exportDiff(values); }}>Export tokens.css diff</Button>
    {#if diff}<textarea aria-label="tokens.css diff (select and copy)" readonly value={diff} onfocus={(e) => e.currentTarget.select()}></textarea>{/if}
    <p>Bar height previews CSS only; the native page offset is fixed. Verify a chosen size in the app.</p>
  </section>
{/if}

<style>
  .launcher { position: absolute; bottom: var(--space-2); right: var(--space-3); z-index: 4; }
  .tweak-panel { position: absolute; z-index: 5; bottom: 0; left: var(--space-2); right: 0; max-height: 60%; overflow: auto; padding: var(--space-3); background: var(--color-bg); border-top: 1px solid var(--color-line); font-size: var(--font-size-sm); }
  .toolbar { display: flex; gap: var(--space-2); align-items: center; justify-content: space-between; margin-bottom: var(--space-2); }
  .toolbar:first-child { position: sticky; top: 0; z-index: 1; background: var(--color-bg); }
  p { color: var(--color-muted); margin: var(--space-2) 0; }
  [role="alert"] { color: var(--color-danger); }
  details { border-top: 1px solid var(--color-line); padding: var(--space-2) 0; }
  summary { cursor: pointer; }
  .token { display: grid; gap: var(--space-1); margin: var(--space-2) 0; }
  .token > span { font-family: var(--font-mono); overflow-wrap: anywhere; }
  .colors { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: var(--space-2); }
  label { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-1); }
  input, select, textarea { min-width: 0; box-sizing: border-box; width: 100%; border: 1px solid var(--color-line); border-radius: var(--radius-sm); padding: var(--space-1); background: var(--color-surface); color: var(--color-fg); font: inherit; }
  input, select { min-height: var(--size-control); }
  input[type="color"] { width: var(--size-control); padding: 0; }
  textarea { min-height: calc(var(--size-bar) * 4); margin-top: var(--space-2); font-family: var(--font-mono); }
  small { font: inherit; color: var(--color-muted); }
</style>
