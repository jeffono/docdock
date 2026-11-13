<script lang="ts">
import { onMount } from "svelte";
import { errorText, t } from "../../i18n";
import {
  onPermission,
  type PermissionKind,
  type PermissionPrompt,
  permissionAnswer,
} from "../../ipc";
import Button from "../../ui/Button.svelte";
import type { IconName } from "../../ui/Icon.svelte";
import Notice from "../../ui/Notice.svelte";

/**
 * A site's permission request, over the top of the page while `shown`; `element` is bound for its
 * cutout. Stays mounted outside Browser so no request arrives before it listens.
 */
let {
  shown,
  element = $bindable(),
  onerror,
}: { shown: boolean; element?: HTMLElement; onerror: (error: string) => void } = $props();
const icons: Record<PermissionKind, IconName> = {
  location: "mapPin",
  camera: "camera",
  microphone: "mic",
  clipboardRead: "clipboard",
};
let prompt = $state<PermissionPrompt | null>(null);
let busy = $state(false);
onMount(() => {
  const stop = onPermission((value) => {
    prompt = value;
  });
  return () => void stop.then((unlisten) => unlisten());
});
async function answer(allow: boolean) {
  if (!prompt || busy) return;
  busy = true;
  try {
    await permissionAnswer(prompt.id, allow);
  } catch (e) {
    onerror(errorText(e));
  } finally {
    busy = false;
  }
}
</script>

{#if prompt && shown}
  <div class="permission" bind:this={element}>
    <Notice icon={icons[prompt.kind]}>
      {t(`permission.${prompt.kind}`, { site: prompt.site })}
      {#if prompt.origin}<small>{t("permission.frame", { origin: prompt.origin })}</small>{/if}
      {#snippet actions()}
        <Button variant="neutral" disabled={busy} onclick={() => answer(false)}>{t("permission.block")}</Button>
        <Button variant="neutral" disabled={busy} onclick={() => answer(true)}>{t("permission.allow")}</Button>
      {/snippet}
    </Notice>
  </div>
{/if}

<style>
  /* Floats over the top of the page, which Rust cuts out of the page window. */
  .permission {
    position: absolute;
    z-index: 1;
    top: var(--size-bar);
    left: 0;
    right: 0;
    padding-top: var(--space-2);
    background: var(--color-bg);
    border-bottom: 1px solid var(--color-line);
  }

  small {
    display: block;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }
</style>
