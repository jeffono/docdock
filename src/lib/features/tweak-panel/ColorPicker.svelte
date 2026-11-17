<script lang="ts">
import { type HSB, hsbToRgb, type RGB, rgbToHsb } from "./controls";
import NumberField from "./NumberField.svelte";

let {
  value,
  label,
  onchange,
}: { value: string; label: string; onchange: (value: string) => void } = $props();
let model = $state("HSB");
// Resolve existing CSS spellings without rewriting untouched token values.
const rgba = $derived.by(() => {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 1;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return [0, 0, 0, 255];
  ctx.fillStyle = value;
  ctx.fillRect(0, 0, 1, 1);
  return Array.from(ctx.getImageData(0, 0, 1, 1).data);
});
let hsb = $state<HSB>([0, 0, 0]);
let emitted = "";
$effect(() => {
  if (value !== emitted) hsb = rgbToHsb(rgba.slice(0, 3) as RGB);
});
// Opaque colors keep the six-digit spelling tokens.css uses.
const toHex = (channels: number[]) =>
  `#${(channels[3] === 255 ? channels.slice(0, 3) : channels).map((v) => v.toString(16).padStart(2, "0")).join("")}`;
const hex = $derived(toHex(rgba));
let field: HTMLInputElement;
// The field keeps what is being typed; it shows the resolved color only when not focused.
$effect(() => {
  if (document.activeElement !== field) field.value = hex;
});
// Paths that bypass `emit` must not leave it pointing at an older color.
function apply(color: string) {
  emitted = "";
  onchange(color);
}
function change(index: number, value: number) {
  const next = [...hsb] as HSB;
  next[index] = value;
  hsb = next;
  emit(hsbToRgb(next), rgba[3]);
}
function emit(rgb: RGB, alpha: number) {
  emitted = toHex([...rgb, alpha]);
  onchange(emitted);
}
</script>
<div class="picker">
  <div class="preview"><input type="color" aria-label={`${label} picker`} value={hex.slice(0,7)} oninput={(e) => { apply(`${e.currentTarget.value}${hex.slice(7)}`); }} /><input bind:this={field} aria-label={`${label} hex or CSS color`} oninput={(e) => { if (CSS.supports("color", e.currentTarget.value)) apply(e.currentTarget.value); }} onblur={(e) => { e.currentTarget.value = hex; }} /></div>
  <select aria-label={`${label} color model`} bind:value={model}><option>HSB</option><option>Hex / CSS</option></select>
  {#if model === "HSB"}<div class="channels">{#each ["H", "S", "B"] as channel, i}<label>{channel}<NumberField label={`${label} ${channel}`} value={Number(hsb[i].toFixed(2))} max={i === 0 ? 360 : 100} onchange={(v) => change(i,v)} /></label>{/each}</div>{/if}
  <label>Alpha %<NumberField label={`${label} alpha`} value={Number((rgba[3]/255*100).toFixed(2))} onchange={(v) => emit(rgba.slice(0,3) as RGB, Math.round(v/100*255))} /></label>
</div>
<style>
.picker { display: grid; gap: var(--space-1); min-width: 0; }
.preview { display: flex; gap: var(--space-1); }
.preview input[type="color"] { width: var(--size-control); flex: 0 0 var(--size-control); padding: 0; }
.channels { display: grid; grid-template-columns: repeat(3, minmax(0,1fr)); gap: var(--space-1); }
</style>
