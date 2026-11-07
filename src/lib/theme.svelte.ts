import { appSetTheme, appTheme, type Theme } from "./ipc";

export const appearance = $state<{ theme: Theme }>({ theme: "system" });

function apply(theme: Theme) {
  appearance.theme = theme;
  document.documentElement.style.colorScheme = theme === "system" ? "light dark" : theme;
}

export async function loadTheme() {
  apply(await appTheme().catch(() => "system" as const));
}

export async function chooseTheme(theme: Theme) {
  await appSetTheme(theme);
  apply(theme);
}
