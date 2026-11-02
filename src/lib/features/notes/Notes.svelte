<script lang="ts">
import { onMount } from "svelte";
import {
  type Note,
  type NotesFolder,
  notesChoose,
  notesFolder,
  notesList,
  notesOpen,
  openExternal,
} from "../../ipc";

let folder = $state<NotesFolder | null>(null);
let notes = $state<Note[]>([]);
let content = $state("");
let selected = $state("");
let error = $state("");
async function refresh(value: NotesFolder | null) {
  folder = value;
  notes = [];
  selected = "";
  content = "";
  if (value?.available) notes = await notesList();
}
async function choose() {
  try {
    error = "";
    await refresh(await notesChoose());
  } catch (e) {
    error = String(e);
  }
}
async function open(path: string) {
  try {
    error = "";
    content = await notesOpen(path);
    selected = path;
  } catch (e) {
    error = String(e);
  }
}
function linkClick(event: MouseEvent) {
  const link = (event.target as Element).closest("a");
  if (!link) return;
  event.preventDefault();
  const href = link.getAttribute("href") ?? "";
  if (href.startsWith("#")) document.getElementById(`note-${href.slice(1)}`)?.scrollIntoView();
  else if (/^(https?:|mailto:)/i.test(href))
    void openExternal(href).catch((e) => {
      error = String(e);
    });
}
onMount(() => {
  notesFolder()
    .then(refresh)
    .catch((e) => {
      error = String(e);
    });
});
</script>
<section aria-label="Notas">
  <button onclick={choose}>Escolher pasta de notas</button>
  {#if folder}
    <p>{folder.path}</p>
    {#if !folder.available}<p>A pasta de notas não está disponível. Escolha outra pasta.</p>{/if}
  {:else}<p>Escolha uma pasta para consultar suas notas.</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  {#if folder?.available && notes.length === 0}<p>Nenhuma nota encontrada.</p>{/if}
  <ul>{#each notes as note}<li><button onclick={() => open(note.path)} title={note.path}>{note.name}</button></li>{/each}</ul>
  {#if selected}<h2>{selected}</h2><div class="note" onclick={linkClick} role="presentation">
    <!-- O HTML já foi sanitizado no Rust. -->
    {@html content}
  </div>{/if}
</section>
<style>
  section { overflow: auto; }
  p, h2 { margin: var(--space-2) 0; font-size: var(--font-size-sm); overflow-wrap: anywhere; }
  ul { padding-left: var(--space-4); }
  .note { overflow-wrap: anywhere; }
  .note :global(img) { max-width: 100%; }
  .note :global(table), .note :global(pre) { display: block; overflow-x: auto; }
</style>
