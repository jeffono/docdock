import { mount } from "svelte";
import App from "./App.svelte";
import { setLocale } from "./lib/i18n";
import { appLocale } from "./lib/ipc";
import "./lib/styles/tokens.css";

// English stays if Rust cannot answer: the panel must still open.
setLocale(await appLocale().catch(() => "en" as const));
mount(App, { target: document.body });
