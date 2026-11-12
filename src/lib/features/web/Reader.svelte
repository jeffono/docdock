<script lang="ts">
import { t } from "../../i18n";
import type { PageArticle } from "../../ipc";
import "../notes/content.css";
import { articleNote } from "../notes/page";
import { renderNote } from "../notes/render";

let { article, onlink }: { article: PageArticle; onlink: (event: MouseEvent) => boolean } =
  $props();
</script>

<!-- The article is untrusted Markdown, built by the note model as DOM nodes: never HTML, and
     without a note path, so it shows no local image. -->
<div class="reader" role="region" aria-label={t("web.reader")}>
  <div
    class="note-content"
    onclick={onlink}
    role="presentation"
    {@attach (element) => {
      let current = true;
      void renderNote(articleNote(article).text, null).then((fragment) => {
        if (current) element.replaceChildren(fragment);
      });
      return () => { current = false; };
    }}
  ></div>
</div>

<style>
  .reader {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-3) var(--panel-inset-right) var(--space-4) var(--panel-inset-left);
    overflow-wrap: anywhere;
  }

  .note-content > :global(:first-child) {
    margin-top: 0;
  }
</style>
