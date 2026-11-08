<script lang="ts">
import type { Snippet } from "svelte";

/**
 * A menu inside the panel, never a native popup. Items are buttons with a `menuitem` role in
 * `children`; choosing one closes the menu and returns focus before its own click runs, so an item
 * may move the focus elsewhere. Arrows, Home and End move between items; Esc and Tab close it.
 */
let {
  id,
  label,
  class: className = "",
  ontoggle,
  children,
}: {
  id?: string;
  label: string;
  /** Positions the menu, such as with a CSS anchor; without it, `show` places it at a point. */
  class?: string;
  ontoggle?: (open: boolean) => void;
  children: Snippet;
} = $props();
let menu: HTMLDivElement;
let returnTo: HTMLElement | null = null;

const items = () => [
  ...menu.querySelectorAll<HTMLButtonElement>("[role^='menuitem']:not(:disabled)"),
];

/**
 * Opens with focus on the checked item, else the first, or on the last. `at` places the menu at a
 * point in the viewport, kept inside it; `from` takes the focus back when the menu closes.
 */
export function show({
  last = false,
  at,
  from,
}: {
  last?: boolean;
  at?: { x: number; y: number };
  from?: HTMLElement | null;
} = {}) {
  returnTo = from ?? null;
  menu.showPopover();
  if (at) {
    const { width, height } = menu.getBoundingClientRect();
    const margin = Number.parseFloat(getComputedStyle(menu).marginTop) || 0;
    const left = Math.max(margin, Math.min(at.x, innerWidth - width - margin));
    const top = at.y + height + margin <= innerHeight ? at.y : Math.max(margin, at.y - height);
    menu.style.left = `${left}px`;
    menu.style.top = `${top}px`;
  }
  const found = items();
  (last
    ? found.at(-1)
    : (menu.querySelector<HTMLButtonElement>('[aria-checked="true"]') ?? found[0])
  )?.focus();
}

export function close() {
  menu.hidePopover();
  returnTo?.focus({ preventScroll: true });
  returnTo = null;
}

function key(event: KeyboardEvent) {
  event.stopPropagation();
  const found = items();
  const at = found.indexOf(document.activeElement as HTMLButtonElement);
  if (event.key === "Escape" || event.key === "Tab") {
    event.preventDefault();
    close();
  } else if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    found[at]?.click();
  } else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
    event.preventDefault();
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? found.length - 1
          : (at + (event.key === "ArrowDown" ? 1 : found.length - 1)) % found.length;
    found[next]?.focus();
  }
}

function choose(event: MouseEvent) {
  const item = (event.target as Element).closest<HTMLButtonElement>("[role^='menuitem']");
  if (item && !item.disabled) close();
}
</script>

<div bind:this={menu} {id} class="menu {className}" popover="auto" role="menu" tabindex="-1" aria-label={label} onkeydown={key} onclickcapture={choose} ontoggle={() => ontoggle?.(menu.matches(":popover-open"))}>
  {@render children()}
</div>

<style>
.menu { position: fixed; inset: auto; margin: var(--space-1) 0; padding: var(--space-1); width: max-content; max-width: calc(100dvw - var(--space-4) * 2); max-height: calc(100dvh - var(--space-4) * 2); overflow: auto; box-sizing: border-box; border: 1px solid var(--color-line); border-radius: var(--radius-sm); background: var(--color-surface); color: var(--color-fg); }
.menu :global([role^="menuitem"]) { display: flex; align-items: center; gap: var(--space-2); width: 100%; min-height: var(--size-control); padding: var(--space-1) var(--space-2); border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--color-fg); font: inherit; font-size: var(--font-size-sm); text-align: left; }
.menu :global([role^="menuitem"]:hover), .menu :global([role^="menuitem"]:focus-visible) { background: var(--color-bg); }
.menu :global([role^="menuitem"]:disabled) { color: var(--color-muted); cursor: default; }
.menu :global(.check) { flex: none; width: var(--size-icon); }
.menu :global(kbd) { margin-left: auto; padding-left: var(--space-4); color: var(--color-muted); font: inherit; }
.menu :global(hr) { border: 0; border-top: 1px solid var(--color-line); margin: var(--space-1) 0; }
.menu :global(p) { color: var(--color-muted); font-size: var(--font-size-sm); margin: var(--space-1) var(--space-2); max-width: calc(var(--size-control) * 8); }
</style>
