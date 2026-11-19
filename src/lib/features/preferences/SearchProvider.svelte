<script lang="ts">
import { onMount } from "svelte";
import { errorText, t } from "../../i18n";
import {
  type SearchEngine,
  type SearchPreferences,
  searchEngines,
  webSearchPreferences,
  webSetSearchPreferences,
} from "../../ipc";
import Button from "../../ui/Button.svelte";
import TextField from "../../ui/TextField.svelte";

const id = $props.id();
const names = {
  duckduckgo: "DuckDuckGo",
  brave: "Brave",
  startpage: "Startpage",
  ecosia: "Ecosia",
  google: "Google",
  bing: "Bing",
};
let settings = $state<SearchPreferences>();
let selected = $state<SearchEngine>();
let draft = $state("");
let busy = $state(false);
let error = $state("");

function apply(value: SearchPreferences) {
  settings = value;
  selected = value.engine;
  draft = value.customUrl;
}

onMount(() => {
  void webSearchPreferences()
    .then(apply)
    .catch((e) => {
      error = errorText(e);
    });
});

async function save(engine: SearchEngine, url: string) {
  if (busy) return;
  busy = true;
  error = "";
  try {
    apply(await webSetSearchPreferences(engine, url));
  } catch (e) {
    error = errorText(e);
    selected = settings?.engine;
  } finally {
    busy = false;
  }
}
</script>

<fieldset aria-busy={busy}>
  <legend>{t("preferences.engine")}</legend>
  <div class="providers">
    {#each searchEngines as engine (engine)}
      <label>
        <input type="radio" name={id} value={engine} bind:group={selected} disabled={!settings} onchange={() => { if (settings) void save(engine, settings.customUrl); }} />
        {engine === "custom" ? t("preferences.engineCustom") : names[engine]}
      </label>
    {/each}
  </div>
  {#if settings?.engine === "custom"}
    <form onsubmit={(event) => { event.preventDefault(); void save("custom", draft); }}>
      <label for={`${id}-url`}>{t("preferences.engineUrl")}</label>
      <div class="url">
        <TextField id={`${id}-url`} bind:value={draft} readonly={busy} placeholder="https://example.com/search?q=%s" spellcheck={false} aria-invalid={settings.invalidCustom && draft === settings.customUrl} aria-describedby={settings.invalidCustom ? `${id}-hint` : undefined} />
        <Button type="submit" variant="neutral" disabled={busy || draft === settings.customUrl}>{t("preferences.engineSave")}</Button>
      </div>
    </form>
    {#if settings.invalidCustom}<p id={`${id}-hint`} role="status">{t("preferences.engineInvalid")}</p>{/if}
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</fieldset>

<style>
fieldset { border: 0; min-width: 0; margin: 0; padding: 0; }
legend { font-size: var(--font-size-md); font-weight: 600; margin-bottom: var(--space-2); padding: 0; }
.providers { display: flex; flex-wrap: wrap; gap: var(--space-2) var(--space-4); }
.providers label { display: inline-flex; align-items: center; gap: var(--space-1); }
input { accent-color: var(--color-accent); }
form { margin-top: var(--space-3); }
.url { display: flex; gap: var(--space-2); margin-top: var(--space-1); }
p { color: var(--color-muted); margin: var(--space-2) 0 0; }
.error { color: var(--color-danger); }
</style>
