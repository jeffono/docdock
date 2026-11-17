<script lang="ts">
import { stepValue } from "./controls";

let {
  value,
  min = 0,
  max = 100,
  step = 1,
  label,
  onchange,
}: {
  value: number;
  min?: number;
  max?: number;
  step?: number;
  label: string;
  onchange: (value: number) => void;
} = $props();
function move(direction: number, shift = false) {
  onchange(stepValue(value, direction, step, min, max, shift));
}
</script>
<div class="number">
  <input type="number" aria-label={label} {value} {min} {max} {step} oninput={(e) => { if (Number.isFinite(e.currentTarget.valueAsNumber)) { const next = Math.min(max, Math.max(min, e.currentTarget.valueAsNumber)); if (next !== e.currentTarget.valueAsNumber) e.currentTarget.value = String(next); onchange(next); } }} onblur={(e) => { e.currentTarget.value = String(value); }} onkeydown={(e) => { if (e.key === "ArrowUp" || e.key === "ArrowDown") { e.preventDefault(); move(e.key === "ArrowUp" ? 1 : -1, e.shiftKey); } }} />
  <div class="steps"><button type="button" aria-label={`Increase ${label}`} onclick={(e) => move(1, e.shiftKey)}>&#9650;</button><button type="button" aria-label={`Decrease ${label}`} onclick={(e) => move(-1, e.shiftKey)}>&#9660;</button></div>
</div>
<style>
.number { display: flex; min-width: 0; width: 100%; }
input { appearance: textfield; }
input::-webkit-inner-spin-button { appearance: none; }
.steps { display: grid; }
button { background: var(--color-surface); color: var(--color-fg); border: 1px solid var(--color-line); padding: 0 var(--space-1); font-size: var(--font-size-sm); cursor: pointer; }
</style>
