<script lang="ts">
import { errorText, t } from "../../i18n";
import { panelSetEditing, webOpen } from "../../ipc";
import Button from "../../ui/Button.svelte";
import TextField from "../../ui/TextField.svelte";

let url = $state("");
let error = $state("");

async function open(event: SubmitEvent) {
  event.preventDefault();
  try {
    error = "";
    await webOpen(url);
  } catch (e) {
    error = errorText(e);
  }
}
</script>

<form onsubmit={open}>
  <TextField
    bind:value={url}
    aria-label={t("web.address")}
    placeholder="https://…"
    onfocus={() => panelSetEditing(true)}
    onblur={() => panelSetEditing(false)}
  />
  <Button type="submit">{t("web.open")}</Button>
</form>
{#if error}
  <p role="alert">{error}</p>
{/if}

<style>
  form {
    display: flex;
    flex: none;
    gap: var(--space-2);
    align-items: center;
    box-sizing: border-box;
    height: var(--size-bar);
    padding: 0 var(--panel-inset-right) 0 var(--panel-inset-left);
    border-bottom: 1px solid var(--color-line);
  }

  p {
    margin: 0;
    padding: var(--space-2) var(--panel-inset-right) var(--space-2) var(--panel-inset-left);
    color: var(--color-danger);
    font-size: var(--font-size-sm);
    overflow-wrap: anywhere;
  }
</style>
