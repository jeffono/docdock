export type RGB = [number, number, number];
export type HSB = [number, number, number];

export function rgbToHsb([r, g, b]: RGB): HSB {
  const max = Math.max(r, g, b),
    min = Math.min(r, g, b),
    delta = max - min;
  const hue =
    delta === 0
      ? 0
      : max === r
        ? ((g - b) / delta) % 6
        : max === g
          ? (b - r) / delta + 2
          : (r - g) / delta + 4;
  return [(hue * 60 + 360) % 360, max === 0 ? 0 : (delta / max) * 100, (max / 255) * 100];
}

export function hsbToRgb([h, s, b]: HSB): RGB {
  const v = Math.min(100, Math.max(0, b)) / 100;
  const c = (v * Math.min(100, Math.max(0, s))) / 100;
  const sector = (((h % 360) + 360) % 360) / 60;
  const x = c * (1 - Math.abs((sector % 2) - 1));
  const channels =
    sector < 1
      ? [c, x, 0]
      : sector < 2
        ? [x, c, 0]
        : sector < 3
          ? [0, c, x]
          : sector < 4
            ? [0, x, c]
            : sector < 5
              ? [x, 0, c]
              : [c, 0, x];
  return channels.map((channel) => Math.round((channel + v - c) * 255)) as RGB;
}

export function stepValue(
  value: number,
  direction: number,
  step: number,
  min: number,
  max: number,
  shift = false,
) {
  return Math.min(
    max,
    Math.max(min, Number((value + direction * step * (shift ? 10 : 1)).toFixed(8))),
  );
}
