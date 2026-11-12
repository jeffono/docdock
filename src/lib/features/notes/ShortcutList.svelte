<script lang="ts">
import { t } from "../../i18n";
import { activation } from "../preferences/activation.svelte";
import { editorShortcuts, panelShortcuts } from "./shortcuts";
</script>

<h3>{t("preferences.panelKeys")}</h3>
<dl><div><dt>{activation.shortcut ?? t("preferences.activationNone")}</dt><dd>{t("shortcut.global")}</dd></div>{#each panelShortcuts as [key, description]}<div><dt>{key}</dt><dd>{t(description)}</dd></div>{/each}</dl>
<h3>{t("preferences.editorKeys")}</h3>
<dl>{#each editorShortcuts as { key, description, level }}<div><dt>{key}</dt><dd>{t(description, { level })}</dd></div>{/each}</dl>

<style>
h3 { font-size: var(--font-size-md); font-weight: 600; margin: 0 0 var(--space-2); }
dl { margin: 0 0 var(--space-4); font-size: var(--font-size-sm); }
dl div { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-3); padding: var(--space-2) 0; border-bottom: 1px solid var(--color-line); }
dt { overflow-wrap: anywhere; }
dd { margin: 0; color: var(--color-muted); }
</style>
