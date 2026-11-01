<script lang="ts">
import { appVersion } from "./lib/ipc";
</script>

<main>
  <h1>DocDock</h1>
  {#await appVersion() then version}
    <p>Versão {version}</p>
  {/await}
</main>

<style>
  main {
    padding: var(--space-4);
  }

  h1 {
    margin: 0 0 var(--space-2);
    font-size: var(--font-size-lg);
  }

  p {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }
</style>
