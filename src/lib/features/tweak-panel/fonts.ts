import { devFonts } from "../../ipc";

export const curatedFonts = [
  '"Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif',
  '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif',
  '"Segoe UI", system-ui, sans-serif',
  "Arial, sans-serif",
  "Verdana, sans-serif",
  "Georgia, serif",
  '"Cascadia Mono", Consolas, monospace',
  "Consolas, monospace",
];
let installed: Promise<string[]> | undefined;
export function installedFonts() {
  installed ??= devFonts().catch((error) => {
    installed = undefined;
    throw error;
  });
  return installed;
}

export function fontStack(family: string, mono: boolean) {
  const quoted = Array.from(family, (char) => {
    const code = char.charCodeAt(0);
    return code < 32 || code === 127 || char === '"' || code === 92
      ? `\\${code.toString(16)} `
      : char;
  }).join("");
  return `"${quoted}", ${mono ? '"Cascadia Mono", Consolas, monospace' : '"Segoe UI", system-ui, sans-serif'}`;
}
