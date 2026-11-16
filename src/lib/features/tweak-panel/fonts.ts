import { onTweakFonts, tweakFontsRequest } from "../../ipc";

/** Offered first, before the installed families. */
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
/** The installed families, asked once of the panel: the tweak window cannot invoke `dev_fonts`. */
export function installedFonts() {
  installed ??= askPanel().catch((error) => {
    installed = undefined;
    throw error;
  });
  return installed;
}

async function askPanel() {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  let unlisten: (() => void) | undefined;
  try {
    return await new Promise<string[]>((resolve, reject) => {
      // A panel reloading at that moment never answers: give up, and the next picker asks again.
      timeout = setTimeout(() => reject(new Error("The panel did not answer")), 5000);
      onTweakFonts((reply) => {
        if ("families" in reply) resolve(reply.families);
        else reject(new Error(reply.error));
      })
        .then((stop) => {
          unlisten = stop;
          return tweakFontsRequest();
        })
        .catch(reject);
    });
  } finally {
    clearTimeout(timeout);
    unlisten?.();
  }
}

/** A CSS stack for an installed `family`, quoted and escaped, with a matching fallback. */
export function fontStack(family: string, mono: boolean) {
  const quoted = Array.from(family, (char) => {
    const code = char.charCodeAt(0);
    return code < 32 || code === 127 || char === '"' || code === 92
      ? `\\${code.toString(16)} `
      : char;
  }).join("");
  return `"${quoted}", ${mono ? '"Cascadia Mono", Consolas, monospace' : '"Segoe UI", system-ui, sans-serif'}`;
}
