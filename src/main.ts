import { mount } from "svelte";
import App from "./App.svelte";
import { keepCopiesPrivate } from "./lib/clipboard";
import { setLocale } from "./lib/i18n";
import { appLocale, clipboardWrite } from "./lib/ipc";
import { loadTheme } from "./lib/theme.svelte";
import "./lib/styles/tokens.css";

// English stays if Rust cannot answer: the panel must still open.
const [locale] = await Promise.all([appLocale().catch(() => "en" as const), loadTheme()]);
setLocale(locale);
keepCopiesPrivate(clipboardWrite);
mount(App, { target: document.body });
