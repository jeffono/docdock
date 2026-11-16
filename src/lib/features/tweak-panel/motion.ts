import source from "../../../../src-tauri/src/motion.rs?raw";
import type { Motions } from "../../ipc";
import { unifiedDiff } from "./diff";

export type { Curve, Motion, Motions } from "../../ipc";

/** Each kind of slide with its release constant in `motion.rs`. */
export const kinds = [
  { key: "open", constant: "OPEN", label: "Panel open" },
  { key: "close", constant: "CLOSE", label: "Panel close" },
  { key: "handleIn", constant: "HANDLE_IN", label: "Handle slide in" },
  { key: "handleOut", constant: "HANDLE_OUT", label: "Handle slide out" },
] as const;

/** Named `cubic-bezier` curves; the `back` ones overshoot. */
export const curves: Record<string, Motions["open"]["curve"]> = {
  linear: [0, 0, 1, 1],
  ease: [0.25, 0.1, 0.25, 1],
  "ease-in": [0.42, 0, 1, 1],
  "ease-out": [0, 0, 0.58, 1],
  "ease-in-out": [0.42, 0, 0.58, 1],
  "ease-in-out cubic": [0.65, 0, 0.35, 1],
  "ease-out cubic": [0.33, 1, 0.68, 1],
  "back out": [0.34, 1.56, 0.64, 1],
  "back in": [0.36, 0, 0.66, -0.56],
  "back in-out": [0.68, -0.6, 0.32, 1.6],
};

/** The same limits `motion.rs` enforces: x within 0..=1 keeps the curve a function of time. */
export const limits = { x: [0, 1], y: [-1, 2], duration: [0, 2000] } as const;

export function validMotion(motion: unknown): motion is Motions["open"] {
  if (!motion || typeof motion !== "object") return false;
  const { duration, curve } = motion as Record<string, unknown>;
  const within = (value: unknown, [min, max]: readonly [number, number]) =>
    typeof value === "number" && Number.isFinite(value) && value >= min && value <= max;
  return (
    Number.isInteger(duration) &&
    within(duration, limits.duration) &&
    Array.isArray(curve) &&
    curve.length === 4 &&
    curve.every((value, i) => within(value, i % 2 === 0 ? limits.x : limits.y))
  );
}

const pattern = (constant: string) =>
  new RegExp(`^const ${constant}: Motion = Motion::new\\((\\d+), \\[([^\\]]+)\\]\\);$`);

/** The release motions as `motion.rs` spells them. */
export function parseMotions(text = source): Motions {
  const lines = text.split(/\r?\n/);
  const entries = kinds.map(({ key, constant }) => {
    const match = lines.map((line) => pattern(constant).exec(line)).find(Boolean);
    if (!match) throw new Error(`Cannot find ${constant} in motion.rs`);
    const curve = match[2].split(",").map(Number) as Motions["open"]["curve"];
    return [key, { duration: Number(match[1]), curve }];
  });
  return Object.fromEntries(entries) as Motions;
}

export const motionDefaults = parseMotions();

/** A Rust `f64` literal: `0.0`, not `0`. */
function float(value: number) {
  const rounded = Math.round(value * 1000) / 1000;
  return Number.isInteger(rounded) ? rounded.toFixed(1) : String(rounded);
}

/** The `motion.rs` constants to change for `values`, as a unified diff. */
export function exportMotion(values: Motions, text = source) {
  const lines = text.split(/\r?\n/);
  const edited = lines.map((line) => {
    const kind = kinds.find(({ constant }) => pattern(constant).test(line));
    if (!kind) return line;
    const { duration, curve } = values[kind.key];
    return `const ${kind.constant}: Motion = Motion::new(${duration}, [${curve.map(float).join(", ")}]);`;
  });
  return unifiedDiff("src-tauri/src/motion.rs", lines, edited) ?? "No motion changes.";
}

/** The preset matching `curve`, or an empty string. */
export function curveName(curve: Motions["open"]["curve"]) {
  return (
    Object.entries(curves).find(([, points]) =>
      points.every((value, i) => value === curve[i]),
    )?.[0] ?? ""
  );
}
