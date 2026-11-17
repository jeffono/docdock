<script lang="ts">
import Button from "../../ui/Button.svelte";
import CurveEditor from "./CurveEditor.svelte";
import { type Curve, curveName, curves, kinds, limits, type Motion, validMotion } from "./motion";
import NumberField from "./NumberField.svelte";
import { status, tweak } from "./state.svelte";

function set(key: (typeof kinds)[number]["key"], motion: Motion) {
  if (!validMotion(motion)) {
    status.error = `Invalid motion: duration within ${limits.duration.join("..")} ms, x within 0..1, y within -1..2.`;
    return;
  }
  tweak.motion[key] = motion;
  status.error = "";
}
function point(curve: Curve, index: number, value: number): Curve {
  const next = [...curve] as Curve;
  next[index] = value;
  return next;
}
</script>

<details><summary>Motion - native slides</summary>
  <p>Edits apply while typing. Each slide reads its motion when it starts; an ongoing slide finishes with its starting values. CSS transitions use the separate --duration token. Release builds keep the constants in motion.rs.</p>
  {#each kinds as { key, label }}
    {@const motion = tweak.motion[key]}
    <fieldset>
      <legend>{label}</legend>
      <label class="token">Duration (ms)<NumberField label={`${label} duration`} min={limits.duration[0]} max={limits.duration[1]} step={10} value={motion.duration} onchange={(duration) => set(key, { ...motion, duration })} /></label>
      <label class="token">Curve
        <select value={curveName(motion.curve)} onchange={(e) => { if (e.currentTarget.value in curves) set(key, { ...motion, curve: [...curves[e.currentTarget.value]] as Curve }); }}>
          <option value="">Custom</option>
          {#each Object.keys(curves) as name}<option value={name}>{name}</option>{/each}
        </select>
      </label>
      <div class="points">
        {#each ["x1", "y1", "x2", "y2"] as name, i}
          <label>{name}<NumberField label={`${label} ${name}`} step={0.01} min={i % 2 === 0 ? limits.x[0] : limits.y[0]} max={i % 2 === 0 ? limits.x[1] : limits.y[1]} value={motion.curve[i]} onchange={(value) => set(key, { ...motion, curve: point(motion.curve, i, value) })} /></label>
        {/each}
      </div>
      <CurveEditor curve={motion.curve} label={`${label} curve`} onchange={(curve) => set(key, { ...motion, curve })} />
      <Button variant="neutral" onclick={() => { for (const other of kinds) tweak.motion[other.key] = $state.snapshot(motion); }}>Use for all slides</Button>
    </fieldset>
  {/each}
</details>

<style>
  fieldset { display: grid; gap: var(--space-2); margin: var(--space-2) 0; padding: var(--space-2); border: 1px solid var(--color-line); border-radius: var(--radius-sm); }
  .points { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--space-1); }
</style>
