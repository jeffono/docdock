import source from "../../styles/tokens.css?raw";
import { unifiedDiff } from "./diff";

// Read declarations from the source, preserving their original spelling for export.
const parse = (text: string) =>
  [...text.matchAll(/^\s*(--[\w-]+):\s*([^;]+);/gm)].map(([, name, value]) => ({ name, value }));
export const tokens = parse(source);
export const defaults = Object.fromEntries(tokens.map(({ name, value }) => [name, value]));
export type Values = Record<string, string>;

export function colors(value: string): string[] | null {
  const match = /^light-dark\((.*)\)$/.exec(value);
  if (!match) return null;
  let depth = 0;
  for (let i = 0; i < match[1].length; i++) {
    const char = match[1][i];
    if (char === "(") depth++;
    if (char === ")") depth--;
    if (char === "," && depth === 0)
      return [match[1].slice(0, i).trim(), match[1].slice(i + 1).trim()];
  }
  return null;
}

export function valid(name: string, value: string) {
  if (!value || /[;{}]/.test(value)) return false;
  if (name.startsWith("--color-")) {
    const pair = colors(value);
    return !!pair && pair.every((color) => CSS.supports("color", color));
  }
  const property = name.startsWith("--font-size")
    ? "font-size"
    : name.startsWith("--font-")
      ? "font-family"
      : name.startsWith("--line-height")
        ? "line-height"
        : name === "--tracking-tight"
          ? "letter-spacing"
          : name === "--duration"
            ? "transition-duration"
            : "width";
  return CSS.supports(property, value);
}

export function exportDiff(values: Values, text = source) {
  const changed = parse(text).filter(({ name, value }) => name in values && values[name] !== value);
  if (!changed.length) return "No token changes.";
  const lines = text.split(/\r?\n/);
  const edited = lines.map((line) => {
    const token = changed.find(({ name }) => line.trimStart().startsWith(`${name}:`));
    if (!token) return line;
    const next = line.replace(
      `${token.name}: ${token.value};`,
      `${token.name}: ${values[token.name]};`,
    );
    // An unexpected spelling in the source must not drop an edit silently.
    if (next === line)
      throw new Error(`Cannot export ${token.name}: unexpected spelling in tokens.css`);
    return next;
  });
  return unifiedDiff("src/lib/styles/tokens.css", lines, edited) ?? "No token changes.";
}

// Canvas resolves all browser-supported color formats, including alpha; no inline style is injected.
export function contrast(foreground: string, background: string, canvasColor: string) {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 1;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return null;
  const luminance = (color?: string) => {
    context.clearRect(0, 0, 1, 1);
    for (const fill of [canvasColor, background, color]) {
      if (!fill) continue;
      context.fillStyle = fill;
      context.fillRect(0, 0, 1, 1);
    }
    const rgb = context.getImageData(0, 0, 1, 1).data;
    return [0.2126, 0.7152, 0.0722].reduce((sum, weight, i) => {
      const channel = rgb[i] / 255;
      return (
        sum + weight * (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4)
      );
    }, 0);
  };
  const fg = luminance(foreground);
  const bg = luminance();
  const ratio = (Math.max(fg, bg) + 0.05) / (Math.min(fg, bg) + 0.05);
  return { ratio: ratio.toFixed(2), aa: ratio >= 4.5, aaa: ratio >= 7 };
}
