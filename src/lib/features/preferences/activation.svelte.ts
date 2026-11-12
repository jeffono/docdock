import {
  type ActivationStatus,
  type Combination,
  shortcutActivation,
  shortcutRecord,
  shortcutSet,
} from "../../ipc";

/**
 * The activation shortcut, shared by Preferences, the shortcut sheet and the panel's notice. While
 * `recording`, panel shortcuts stand aside so the recorder receives every key.
 */
export const activation = $state<ActivationStatus & { recording: boolean }>({
  shortcut: null,
  failed: null,
  recording: false,
});

function apply(status: ActivationStatus) {
  activation.shortcut = status.shortcut;
  activation.failed = status.failed;
}

export async function loadActivation() {
  apply(await shortcutActivation());
}

/** Set first, so a blur during the call still ends recording and registers the shortcut again. */
export async function startRecording() {
  activation.recording = true;
  apply(await shortcutRecord(true));
}

export async function stopRecording() {
  if (!activation.recording) return;
  activation.recording = false;
  apply(await shortcutRecord(false));
}

/** Ends recording with `combination`; on failure Rust keeps the previous shortcut. */
export async function finishRecording(combination: Combination) {
  activation.recording = false;
  try {
    apply(await shortcutSet(combination));
  } catch (error) {
    await loadActivation().catch(() => {});
    throw error;
  }
}

export function dismissActivationNotice() {
  activation.failed = null;
}
