<script lang="ts">
import { errorText, t } from "../../i18n";
import { type BlockingStatus, blockingSetAllowed, blockingStatus } from "../../ipc";
import IconButton from "../../ui/IconButton.svelte";
import { blockingRevision } from "./blocking-store";

let { url, onerror }: { url: string; onerror: (error: string) => void } = $props();
let status = $state<BlockingStatus | null>(null);
let busy = $state(false);
$effect(() => {
  const current = url;
  void $blockingRevision;
  status = null;
  let disposed = false;
  void blockingStatus(current)
    .then((value) => {
      if (!disposed) status = value;
    })
    .catch((e) => {
      if (!disposed) onerror(errorText(e));
    });
  return () => {
    disposed = true;
  };
});
async function toggle() {
  if (!status || busy) return;
  busy = true;
  const current = url;
  try {
    await blockingSetAllowed(current, !status.allowed);
    blockingRevision.update((value) => value + 1);
  } catch (e) {
    blockingRevision.update((value) => value + 1);
    onerror(errorText(e));
  } finally {
    busy = false;
  }
}
</script>
{#if status}
  <IconButton quiet icon={status.allowed ? "shieldOff" : "shield"} label={t(status.allowed ? "blocking.allowed" : "blocking.on", { site: status.site })} disabled={busy} onclick={toggle} />
{/if}
