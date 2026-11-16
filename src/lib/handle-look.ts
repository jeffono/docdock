import { type HandleLook, panelHandleLook } from "./ipc";

/** CSS generic families: GDI knows none and would substitute an arbitrary font. */
const GENERIC = new Set([
  "serif",
  "sans-serif",
  "monospace",
  "cursive",
  "fantasy",
  "system-ui",
  "ui-serif",
  "ui-sans-serif",
  "ui-monospace",
  "ui-rounded",
  "emoji",
  "math",
  "fangsong",
]);

/** The named families of a CSS font stack, in order, without quotes or generic families. */
export function fontFamilies(stack: string): string[] {
  return stack
    .split(",")
    .map((family) => family.trim().replace(/^(["'])(.*)\1$/, "$2"))
    .filter((family) => family && !GENERIC.has(family.toLowerCase()));
}

/** A computed sRGB color as its three channels. */
function channels(color: string): [number, number, number] {
  const [red = 0, green = 0, blue = 0] = (color.match(/[\d.]+/g) ?? []).map(Number);
  return [red, green, blue].map(Math.round) as [number, number, number];
}

/** Sends the edge handle the current theme's tokens; call again whenever the theme changes. */
export function reportHandleLook() {
  const probe = document.createElement("span");
  probe.style.color = "var(--color-muted)";
  probe.style.backgroundColor = "var(--color-bg)";
  probe.style.borderColor = "var(--color-line)";
  probe.style.fontFamily = "var(--font-sans)";
  probe.style.fontSize = "var(--font-size-sm)";
  document.body.append(probe);
  const style = getComputedStyle(probe);
  const look: HandleLook = {
    background: channels(style.backgroundColor),
    line: channels(style.borderTopColor),
    text: channels(style.color),
    fonts: fontFamilies(style.fontFamily),
    size: Number.parseFloat(style.fontSize),
  };
  probe.remove();
  // Without it the handle stays hidden, and the edge stretch still opens the panel.
  void panelHandleLook(look).catch(() => {});
}
