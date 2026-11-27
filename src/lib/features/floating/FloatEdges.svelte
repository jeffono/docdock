<script lang="ts">
// Invisible strips along a floating window's content edges and corners, half in its gutter, where
// it resizes: Rust decides what each resize may make of the window.
import { floatResize, type ResizeDirection } from "../../ipc";

let { horizontalOnly = false }: { horizontalOnly?: boolean } = $props();

const edges: [string, ResizeDirection][] = [
  ["n", "North"],
  ["s", "South"],
  ["e", "East"],
  ["w", "West"],
  ["ne", "NorthEast"],
  ["nw", "NorthWest"],
  ["se", "SouthEast"],
  ["sw", "SouthWest"],
];

function resize(event: PointerEvent, direction: ResizeDirection) {
  if (event.button !== 0) return;
  event.preventDefault();
  void floatResize(direction).catch(() => {});
}
</script>

{#each edges.filter(([, direction]) => !horizontalOnly || direction === "East" || direction === "West") as [edge, direction] (edge)}
  <div class="edge {edge}" aria-hidden="true" onpointerdown={(event) => resize(event, direction)}></div>
{/each}

<style>
  /* `--pad` is the window's transparent gutter, set by the window. */
  .edge {
    --edge: calc(1.5 * var(--space-1));
    position: absolute;
  }

  .n,
  .s {
    left: var(--pad);
    right: var(--pad);
    height: calc(2 * var(--edge));
    cursor: ns-resize;
  }

  .e,
  .w {
    top: var(--pad);
    bottom: var(--pad);
    width: calc(2 * var(--edge));
    cursor: ew-resize;
  }

  .ne,
  .nw,
  .se,
  .sw {
    width: calc(2 * var(--edge));
    height: calc(2 * var(--edge));
  }

  .n,
  .ne,
  .nw {
    top: calc(var(--pad) - var(--edge));
  }

  .s,
  .se,
  .sw {
    bottom: calc(var(--pad) - var(--edge));
  }

  .e,
  .ne,
  .se {
    right: calc(var(--pad) - var(--edge));
  }

  .w,
  .nw,
  .sw {
    left: calc(var(--pad) - var(--edge));
  }

  .ne,
  .sw {
    cursor: nesw-resize;
  }

  .nw,
  .se {
    cursor: nwse-resize;
  }
</style>
