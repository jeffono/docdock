<script lang="ts">
import { onMount } from "svelte";
import { errorText, t } from "../../i18n";
import { vaultUnlock } from "../../ipc";
import Button from "../../ui/Button.svelte";
import { loadVault, vaultPath } from "./vault-store";

let busy = $state(false);
let error = $state("");
onMount(() => {
  void loadVault().catch((e) => {
    error = errorText(e);
  });
});
async function unlock() {
  busy = true;
  error = "";
  try {
    await vaultUnlock();
  } catch (e) {
    error = errorText(e);
  } finally {
    busy = false;
  }
}
</script>

{#if $vaultPath}
  <Button disabled={busy} onclick={unlock}>{t("vault.unlock")}</Button>
{/if}
{#if error}<p role="alert">{error}</p>{/if}

<style>
p { color: var(--color-danger); margin: var(--space-2) 0; }
</style>
