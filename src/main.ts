import { mount } from "svelte";
import App from "./App.svelte";
import { keepCopiesPrivate } from "./lib/clipboard";
import { setLocale } from "./lib/i18n";
import {
  appLocale,
  clipboardWrite,
  floatClipboardWrite,
  isFloatWindow,
  isImagePreviewWindow,
  isTweakWindow,
} from "./lib/ipc";
import { loadTheme } from "./lib/theme.svelte";
import "./lib/styles/tokens.css";

if (isImagePreviewWindow()) {
  // Reaches only its own command; the native window theme Rust sets drives `light-dark()`.
  const { default: ImagePreview } = await import(
    "./lib/features/image-preview/ImagePreview.svelte"
  );
  mount(ImagePreview, { target: document.body });
} else if (isFloatWindow()) {
  // Reaches only its own commands, which also answer its language; the native window theme Rust
  // sets drives `light-dark()`.
  const { floatSession } = await import("./lib/features/floating/session");
  // A window closing while it loads answers nothing: there is nothing to show.
  const session = await floatSession().catch(() => null);
  // A post-it's editor copies through its own command, which Rust grants only while it edits.
  if (session) keepCopiesPrivate(floatClipboardWrite);
  if (session?.state.kind === "note") {
    const { default: PostIt } = await import("./lib/features/floating/PostIt.svelte");
    mount(PostIt, { target: document.body, props: { session } });
  } else if (session) {
    const { default: FloatWindow } = await import("./lib/features/floating/FloatWindow.svelte");
    mount(FloatWindow, { target: document.body, props: { session } });
  }
} else if (import.meta.env.DEV && isTweakWindow()) {
  // Development builds only: the tweak window loads the same page and reaches no app command.
  const { default: TweakWindow } = await import("./lib/features/tweak-panel/TweakWindow.svelte");
  mount(TweakWindow, { target: document.body });
} else {
  // English stays if Rust cannot answer: the panel must still open.
  const [locale] = await Promise.all([appLocale().catch(() => "en" as const), loadTheme()]);
  setLocale(locale);
  keepCopiesPrivate(clipboardWrite);
  mount(App, { target: document.body });
}
