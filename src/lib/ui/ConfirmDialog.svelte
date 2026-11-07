<script lang="ts">
import { t } from "../i18n";
import Button from "./Button.svelte";

/**
 * Asks before an action, inside the panel, over a dimmed backdrop. Focus starts on Cancel, so Enter
 * alone never confirms; Esc cancels. Focus returns where it was.
 */
const id = $props.id();
let dialog: HTMLDialogElement;
let question = $state({ title: "", text: "", action: "" });
let answer: ((confirmed: boolean) => void) | null = null;
let previous: Element | null = null;

export function isOpen() {
  return dialog?.open ?? false;
}

/**
 * `near`: what the question is about, such as a card header; the dialog opens just below it, or
 * above when there is no room, inside the panel. Without it, the dialog is centered.
 */
export function ask(title: string, text: string, action: string, near?: DOMRect): Promise<boolean> {
  answer?.(false);
  question = { title, text, action };
  previous = document.activeElement;
  dialog.showModal();
  place(near);
  // Cancel comes first.
  dialog.querySelector("button")?.focus();
  return new Promise((resolve) => {
    answer = resolve;
  });
}

function place(near?: DOMRect) {
  const height = dialog.getBoundingClientRect().height;
  const gap = Number.parseFloat(getComputedStyle(dialog).marginTop) || 0;
  const top = !near
    ? (innerHeight - height) / 2
    : near.bottom + gap + height <= innerHeight - gap
      ? near.bottom
      : near.top - height - 2 * gap;
  // The top margin keeps the gap from what it is about.
  dialog.style.top = `${Math.max(0, Math.min(top, innerHeight - height - 2 * gap))}px`;
}

function done(confirmed: boolean) {
  dialog.close();
  if (previous instanceof HTMLElement && previous.isConnected)
    previous.focus({ preventScroll: true });
  previous = null;
  answer?.(confirmed);
  answer = null;
}
</script>

<dialog bind:this={dialog} aria-labelledby={`${id}-title`} aria-describedby={`${id}-text`} oncancel={(event) => { event.preventDefault(); done(false); }}>
  <h2 id={`${id}-title`}>{question.title}</h2>
  <p id={`${id}-text`}>{question.text}</p>
  <div class="actions">
    <Button variant="neutral" onclick={() => done(false)}>{t("dialog.cancel")}</Button>
    <Button onclick={() => done(true)}>{question.action}</Button>
  </div>
</dialog>

<style>
dialog { box-sizing: border-box; width: calc(100dvw - var(--space-4) * 2); max-width: none; bottom: auto; margin: var(--space-2) auto 0; padding: var(--space-3); border: 1px solid var(--color-line); border-radius: var(--radius-sm); background: var(--color-bg); color: var(--color-fg); }
dialog::backdrop { background: var(--color-scrim); }
h2 { margin: 0 0 var(--space-2); font-size: var(--font-size-md); font-weight: 600; overflow-wrap: anywhere; }
p { margin: 0; font-size: var(--font-size-sm); color: var(--color-muted); }
.actions { display: flex; justify-content: flex-end; gap: var(--space-2); margin-top: var(--space-3); }
</style>
