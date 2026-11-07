<script lang="ts">
import { t } from "../../i18n";
import IconButton from "../../ui/IconButton.svelte";
import ShortcutList from "./ShortcutList.svelte";

const id = $props.id();
let dialog: HTMLDialogElement;
let previous: HTMLElement | null = null;

export function isOpen() {
  return dialog?.open;
}

export function close() {
  if (!dialog?.open) return;
  dialog.close();
  if (previous?.isConnected) previous.focus({ preventScroll: true });
  previous = null;
}

/** Capture before CodeMirror's Mod-/ comment command, including inside its shadow root. */
export function shortcut(event: KeyboardEvent) {
  if (event.isComposing || event.defaultPrevented) return;
  const toggle =
    event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey && event.key === "/";
  if (!toggle && !(dialog.open && event.key === "Escape")) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  if (event.repeat) return;
  if (dialog.open) close();
  else {
    previous =
      event.composedPath().find((node): node is HTMLElement => node instanceof HTMLElement) ?? null;
    dialog.showModal();
  }
}
</script>

<dialog bind:this={dialog} aria-labelledby={id} aria-modal="true" oncancel={(event) => { event.preventDefault(); close(); }}>
  <header><h2 {id}>{t("preferences.shortcuts")}</h2><IconButton icon="x" quiet label={t("search.close")} onclick={close} /></header>
  <div class="content"><ShortcutList /></div>
</dialog>

<style>
dialog { box-sizing: border-box; width: calc(100dvw - var(--space-4) * 2); max-width: none; max-height: calc(100dvh - var(--space-4) * 2); padding: 0; border: 1px solid var(--color-line); border-radius: var(--radius-sm); background: var(--color-bg); color: var(--color-fg); }
dialog[open] { display: flex; flex-direction: column; }
dialog::backdrop { background: transparent; }
header { display: flex; flex: none; align-items: center; justify-content: space-between; gap: var(--space-2); padding: var(--space-2) var(--space-3); border-bottom: 1px solid var(--color-line); }
h2 { font-size: var(--font-size-md); font-weight: 600; margin: 0; }
.content { overflow: auto; padding: var(--space-3); }
</style>
