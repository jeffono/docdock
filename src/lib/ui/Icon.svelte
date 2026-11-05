<script lang="ts">
// Lucide paths (ISC license). Only add icons used by the UI here.
const paths = {
  folder:
    "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
  globe:
    "M12 2a10 10 0 1 0 0 20a10 10 0 0 0 0-20z M2 12h20 M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z",
  pencil:
    "M21.174 6.812a1 1 0 0 0-3.986-3.986L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.623l4.353-1.32a2 2 0 0 0 .83-.5z M15 5l4 4",
  restore: "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8 M3 3v5h5",
  pin: "M12 17v5 M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z",
  bookmark: "m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z",
  chevronDown: "m6 9 6 6 6-6",
  chevronUp: "m18 15-6-6-6 6",
  x: "M18 6 6 18 M6 6l12 12",
};

let { name, filled = false }: { name: keyof typeof paths; filled?: boolean } = $props();
</script>

<svg class:filled viewBox="0 0 24 24" aria-hidden="true"><path d={paths[name]} /></svg>

<style>
  svg {
    width: var(--size-icon);
    height: var(--size-icon);
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  svg.filled {
    fill: currentColor;
  }
</style>
