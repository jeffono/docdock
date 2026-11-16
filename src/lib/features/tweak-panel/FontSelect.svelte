<script lang="ts">
let {
  name,
  value,
  initial,
  onchange,
}: { name: string; value: string; initial: string; onchange: (value: string) => void } = $props();

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
</script>

<select aria-label={name} {value} onchange={(e) => onchange(e.currentTarget.value)}>
  {#each [...new Set([initial, value, ...fonts])] as font}<option value={font}>{font.split(",")[0]}</option>{/each}
</select>
