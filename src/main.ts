import { mount } from "svelte";
import App from "./App.svelte";
import { setLocale } from "./lib/i18n";
import { appLocale } from "./lib/ipc";
import { loadTheme } from "./lib/theme.svelte";
import "./lib/styles/tokens.css";

// English stays if Rust cannot answer: the panel must still open.
const [locale] = await Promise.all([appLocale().catch(() => "en" as const), loadTheme()]);
setLocale(locale);
mount(App, { target: document.body });
