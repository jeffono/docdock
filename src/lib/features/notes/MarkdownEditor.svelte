<script lang="ts">
import { isolateHistory, undo as runUndo } from "@codemirror/commands";
import { Compartment, EditorSelection, EditorState, Prec, type Text } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { onMount, tick, untrack } from "svelte";
import { privateCut } from "../../clipboard";
import { t } from "../../i18n";
import { clipboardWrite, spellingAdd, spellingCheck, spellingSuggest } from "../../ipc";
import Menu from "../../ui/Menu.svelte";
import contentCssUrl from "./content.css?url";
import { type Counts, countSelection, countText, noteBodyStart } from "./count";
import { editorExtensions, editorState } from "./editor";
import cssUrl from "./editor.css?url";
import { editorLinkAt, editorLinkClick } from "./editor-links";
import { linkAction, linkMenuKey } from "./link-actions";
import { notePath } from "./live-formatting";
import NoteStats from "./NoteStats.svelte";
import { diskReload, exactText, rawHistory, rawText } from "./reload";
import { learned, type Misspelling, spelling } from "./spelling";
import { loadSpelling, spellingOn } from "./spelling-store";

let {
  initialText,
  label,
  path = null,
  frontmatter = true,
  leaveOnCtrlE = true,
  placeholder,
  fill = false,
  autofocus = true,
  tags = () => [],
  onleave = () => {},
  onblur,
  onchange,
  onlinkexternal,
  onlinkmenu,
  onpaste,
  ondragover,
  ondropcapture,
  onspellerror,
  onready,
}: {
  initialText: string;
  label: string;
  path?: string | null;
  frontmatter?: boolean;
  leaveOnCtrlE?: boolean;
  placeholder?: string;
  fill?: boolean;
  autofocus?: boolean;
  /** The folder's tags, offered while typing them in the frontmatter. */
  tags?: () => string[];
  onleave?: () => void;
  onblur?: () => void;
  onchange?: (raw: string) => void;
  onlinkexternal?: (url: URL) => void;
  onlinkmenu?: (
    event: MouseEvent | KeyboardEvent,
    href: string,
    from: HTMLElement,
    at?: { x: number; y: number },
  ) => void;
  onpaste?: (event: ClipboardEvent, view: EditorView) => boolean;
  ondragover?: (event: DragEvent) => void;
  ondropcapture?: (event: DragEvent) => void;
  onspellerror?: (error: unknown) => void;
  onready?: (view: EditorView, styled: Promise<unknown>) => void;
} = $props();

let host: HTMLDivElement;
let view: EditorView | undefined;
let raw = untrack(() => initialText);
let total = $state<Counts | null>(null);
let selected = $state<Counts | null>(null);
let pendingSelection: Counts | null = null;
let bodyStart = 0;
let countTimer: ReturnType<typeof setTimeout> | undefined;
let disposed = false;
let replacing = false;

function followLink(event: MouseEvent, editorView: EditorView) {
  if (linkAction(event, false) !== "external") return false;
  const url = editorLinkClick(editorView, event, bodyStart);
  if (!url) return false;
  // Keep mouse-down from selecting text or starting middle-button autoscroll.
  if (event.type !== "mousedown") onlinkexternal?.(url);
  return true;
}

const linkHandlers = Prec.high(
  EditorView.domEventHandlers({
    mousedown: followLink,
    click: followLink,
    auxclick: followLink,
    contextmenu(event, editorView) {
      if (!onlinkmenu) return false;
      const url = editorLinkClick(editorView, event, bodyStart);
      if (!url) return false;
      onlinkmenu(event, url.href, editorView.contentDOM);
      return true;
    },
    keydown(event, editorView) {
      if (!onlinkmenu || event.isComposing || !linkMenuKey(event)) return false;
      const { head } = editorView.state.selection.main;
      if (head < bodyStart) return false;
      const url =
        editorLinkAt(editorView.state, head, 1) ?? editorLinkAt(editorView.state, head, -1);
      const rect = url && editorView.coordsAtPos(head);
      if (!url || !rect) return false;
      onlinkmenu(event, url.href, editorView.contentDOM, { x: rect.left, y: rect.bottom });
      return true;
    },
  }),
);

const editable = new Compartment();
let isReadOnly = false;
const location = new Compartment();
const checker = new Compartment();
const spellingExtension = spelling(spellingCheck, (target) => void suggest(target));
let spellMenu: Menu;
/** The misspelled word the suggestions menu is for. */
let misspelling = $state<(Misspelling & { suggestions: string[]; doc: Text }) | null>(null);

/**
 * The text the menu was opened for, still editable: an edit, an image paste (read-only meanwhile) or
 * a closed editor makes its positions stale, and a fix would replace other text.
 */
function current(doc: Text) {
  return !disposed && view !== undefined && view.state.doc === doc && !view.state.readOnly;
}

async function suggest(target: Misspelling) {
  const doc = view?.state.doc;
  if (!doc) return;
  const suggestions = await spellingSuggest(target.word).catch(() => []);
  if (!current(doc)) return;
  misspelling = { ...target, suggestions, doc };
  // Rendered first, so the menu measures its items.
  await tick();
  if (view && current(doc) && view.hasFocus)
    spellMenu.show({ at: { x: target.x, y: target.y }, from: view.contentDOM });
}

function correct(replacement: string) {
  const target = misspelling;
  if (!view || !target || !current(target.doc)) return;
  view.dispatch({
    changes: { from: target.from, to: target.to, insert: replacement },
    selection: { anchor: target.from + replacement.length },
    // Its own undo step: typing right after must not join it.
    annotations: isolateHistory.of("full"),
    userEvent: "input.spelling",
  });
}

async function addWord() {
  const target = misspelling;
  if (!view || !target) return;
  try {
    await spellingAdd(target.word);
    if (!disposed) learned(view, target.word);
  } catch (e) {
    onspellerror?.(e);
  }
}

/**
 * The cursor keeps about three lines from the top and bottom of what shows when it moves; in card
 * mode it never sits under the card's stuck header.
 */
function scrollRoom() {
  const tokens = getComputedStyle(host);
  const rem = Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
  const line =
    Number.parseFloat(tokens.getPropertyValue("--font-size-md")) *
    rem *
    Number.parseFloat(tokens.getPropertyValue("--line-height"));
  const cursor = EditorView.cursorScrollMargin.of({ x: 5, y: Math.round(3 * line) || 5 });
  if (untrack(() => fill)) return [cursor];
  return [
    cursor,
    EditorView.scrollMargins.of(() => {
      const header = host.closest(".card")?.querySelector<HTMLElement>(":scope > header");
      return { top: header?.offsetHeight ?? 0 };
    }),
  ];
}

function extensions() {
  const withFrontmatter = untrack(() => frontmatter);
  const withCtrlE = untrack(() => leaveOnCtrlE);
  const initialPath = untrack(() => path);
  const placeholderText = untrack(() => placeholder);
  return [
    rawText.init(() => raw),
    rawHistory,
    scrollRoom(),
    EditorView.updateListener.of(({ state, docChanged, selectionSet }) => {
      if (docChanged) {
        bodyStart = withFrontmatter ? noteBodyStart(state.doc) : 0;
        clearTimeout(countTimer);
        const from = bodyStart;
        countTimer = setTimeout(() => {
          total = countText(state.sliceDoc(from));
          selected = pendingSelection;
          countTimer = undefined;
        }, 150);
      }
      if (docChanged || selectionSet) {
        pendingSelection = countSelection(state.doc, state.selection, bodyStart);
        if (countTimer === undefined) selected = pendingSelection;
      }
    }),
    EditorView.updateListener.of((update) => {
      const next = update.state.field(rawText);
      if (next === raw || replacing) return;
      raw = next;
      onchange?.(raw);
    }),
    editable.of(EditorState.readOnly.of(isReadOnly)),
    location.of(notePath.of(initialPath)),
    checker.of([]),
    ...(onlinkexternal || onlinkmenu ? [linkHandlers] : []),
    ...(onpaste
      ? [EditorView.domEventHandlers({ paste: (event, editorView) => onpaste(event, editorView) })]
      : []),
    privateCut(clipboardWrite),
    ...editorExtensions(
      onleave,
      (focused) => {
        if (!focused) onblur?.();
      },
      untrack(() => label),
      tags,
      { frontmatter: withFrontmatter, leaveOnCtrlE: withCtrlE, placeholder: placeholderText },
    ),
  ];
}

export function text(): string {
  return raw;
}

export function getView(): EditorView | undefined {
  return view;
}

export function focus() {
  view?.focus();
}

export function blur() {
  view?.contentDOM.blur();
}

export function measure() {
  view?.requestMeasure();
}

export function hasFocus(): boolean {
  return Boolean(view?.hasFocus);
}

export function setReadOnly(readOnly: boolean) {
  isReadOnly = readOnly;
  view?.dispatch({ effects: editable.reconfigure(EditorState.readOnly.of(readOnly)) });
}

export function setNotePath(next: string | null) {
  view?.dispatch({ effects: location.reconfigure(notePath.of(next)) });
}

export function undo() {
  if (!view) return;
  runUndo(view);
  view.focus();
}

export function clear(options: { refocus?: boolean; allowReadOnly?: boolean } = {}): boolean {
  const { refocus = true, allowReadOnly = false } = options;
  if (!view || (!allowReadOnly && view.state.readOnly) || view.state.doc.length === 0) return false;
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: "" },
    selection: { anchor: 0 },
    annotations: isolateHistory.of("full"),
    userEvent: "delete",
  });
  if (refocus) view.focus();
  return true;
}

/** Replaces text from disk without emitting an edit event. */
export function reloadText(next: string) {
  replacing = true;
  try {
    if (view && next !== raw) view.dispatch(diskReload(view.state, next));
  } finally {
    replacing = false;
  }
  raw = next;
}

/** Dispatches a full-document text edit that records history and triggers `onchange`. */
export function applyEdit(next: string) {
  if (view && next !== raw) view.dispatch(diskReload(view.state, next));
}

/** Appends text at the end as one undoable edit preserving CRLF raw endings. */
export function appendText(edit: {
  from: number;
  to: number;
  insert: string;
  nextRaw: string;
}): boolean {
  if (!view || view.state.readOnly) return false;
  view.dispatch({
    changes: { from: edit.from, to: edit.to, insert: edit.insert },
    effects: exactText.of(edit.nextRaw),
    selection: { anchor: edit.from + edit.insert.length },
    annotations: isolateHistory.of("full"),
    userEvent: "input",
    scrollIntoView: true,
  });
  return true;
}

onMount(() => {
  // The CSP blocks runtime <style> tags. CodeMirror's style-mod (4.1.4) uses an adopted stylesheet
  // only in a root without `head`, so the editor lives in a shadow root; external links load the
  // editor and content CSS inside it, with `:host` specificity above CodeMirror's base styles.
  const root = host.attachShadow({ mode: "open" });
  const css = document.createElement("link");
  css.rel = "stylesheet";
  css.href = cssUrl;
  const contentCss = document.createElement("link");
  contentCss.rel = "stylesheet";
  contentCss.href = contentCssUrl;
  const styled = Promise.all(
    [contentCss, css].map(
      (link) =>
        new Promise((resolve) => {
          link.onload = link.onerror = resolve;
        }),
    ),
  );
  root.append(contentCss, css);
  view = new EditorView({ root, parent: root, state: editorState(raw, extensions()) });
  bodyStart = untrack(() => frontmatter) ? noteBodyStart(view.state.doc) : 0;
  total = countText(view.state.sliceDoc(bodyStart));
  onready?.(view, styled);
  if (untrack(() => autofocus)) view.focus();
  const stopSpelling = spellingOn.subscribe((on) => {
    view?.dispatch({ effects: checker.reconfigure(on ? spellingExtension : []) });
  });
  loadSpelling().catch(() => {});
  return () => {
    stopSpelling();
    clearTimeout(countTimer);
    disposed = true;
    view?.destroy();
    view = undefined;
  };
});
</script>

<div class="editor-core" class:fill>
  <div
    class="surface"
    class:fill
    role="presentation"
    bind:this={host}
    {ondragover}
    {ondropcapture}
  ></div>
  {#if total}
    <div class="stats">
      <NoteStats {total} {selected} />
    </div>
  {/if}
  <Menu bind:this={spellMenu} label={t("editor.spelling")} ontoggle={(open) => { if (!open) misspelling = null; }}>
    {#each misspelling?.suggestions ?? [] as suggestion}
      <button role="menuitem" onclick={() => correct(suggestion)}>{suggestion}</button>
    {:else}
      <p>{t("editor.spellingNone")}</p>
    {/each}
    <hr />
    <button role="menuitem" onclick={addWord}>{t("editor.spellingAdd")}</button>
  </Menu>
</div>

<style>
.editor-core.fill {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.surface.fill {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.editor-core.fill .stats {
  flex: none;
  border-top: 1px solid var(--color-line);
  padding-top: var(--space-1);
}
</style>
