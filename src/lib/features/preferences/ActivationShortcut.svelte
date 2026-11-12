<script lang="ts">
import { onDestroy } from "svelte";
import { errorText, t } from "../../i18n";
import Button from "../../ui/Button.svelte";
import { activation, finishRecording, startRecording, stopRecording } from "./activation.svelte";

const modifierKeys = ["Control", "Alt", "Shift", "Meta", "AltGraph", "OS"];
let error = $state("");
/** Modifiers held while recording, shown before the key completes the combination. */
let held = $state("");

async function run(work: () => Promise<void>) {
  error = "";
  try {
    await work();
  } catch (e) {
    error = errorText(e);
  }
}
const heldText = (event: KeyboardEvent) =>
  [
    event.metaKey && "Win",
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
  ]
    .filter(Boolean)
    .join("+");
function toggle() {
  held = "";
  void run(activation.recording ? stopRecording : startRecording);
}
/** Every key goes to the recorder, except a plain Tab, which moves on and so cancels. */
function key(event: KeyboardEvent) {
  if (!activation.recording) return;
  if (event.key === "Tab" && !event.ctrlKey && !event.altKey && !event.metaKey) return;
  event.preventDefault();
  event.stopPropagation();
  held = heldText(event);
  if (event.isComposing || event.repeat || modifierKeys.includes(event.key)) return;
  if (event.key === "Escape" && !held) {
    void run(stopRecording);
    return;
  }
  const { ctrlKey: ctrl, altKey: alt, shiftKey: shift, metaKey: win, code } = event;
  void run(() => finishRecording({ ctrl, alt, shift, win, code }));
}
function release(event: KeyboardEvent) {
  if (activation.recording) held = heldText(event);
}
onDestroy(() => {
  void stopRecording().catch(() => {});
});
</script>

<div class="setting">
  <h3>{t("preferences.activation")}</h3>
  <div class="row">
    <kbd class:recording={activation.recording} aria-live="polite">
      {#if activation.recording}{held ? `${held}+…` : t("preferences.activationRecording")}{:else}{activation.shortcut ?? t("preferences.activationNone")}{/if}
    </kbd>
    <Button variant="neutral" onclick={toggle} onkeydown={key} onkeyup={release} onblur={() => void run(stopRecording)}>
      {activation.recording ? t("dialog.cancel") : t("preferences.activationChange")}
    </Button>
  </div>
  <p>{t("preferences.activationHint")}</p>
  {#if error}<p role="alert">{error}</p>{/if}
</div>

<style>
.setting { padding-bottom: var(--space-4); }
h3 { font-size: var(--font-size-md); font-weight: 600; margin: 0 0 var(--space-2); }
.row { display: flex; gap: var(--space-2); align-items: center; margin-bottom: var(--space-2); }
kbd { flex: 1; min-width: 0; box-sizing: border-box; height: var(--size-control); display: flex; align-items: center; padding: 0 var(--space-2); border: 1px solid var(--color-line); border-radius: var(--radius-sm); background: var(--color-surface); font: inherit; font-size: var(--font-size-sm); overflow-wrap: anywhere; }
kbd.recording { border-color: var(--color-accent); color: var(--color-muted); }
p { margin: 0 0 var(--space-3); color: var(--color-muted); }
p[role="alert"] { color: var(--color-danger); }
</style>
