// The tweak window's state. Each section owns one key; the panel applies each key in `install.ts`.
import { type HandleTuning, handleDefaults, validHandle } from "./handle";
import { type Motions, motionDefaults, validMotion } from "./motion";
import { type TabTuning, tabDefaults, validTab } from "./tab-shape";
import { defaults as tokenDefaults, type Values, valid } from "./tokens";

export type TweakState = { tokens: Values; motion: Motions; handle: HandleTuning; tab: TabTuning };

function initial(): TweakState {
  return {
    tokens: { ...tokenDefaults },
    motion: structuredClone(motionDefaults),
    handle: { ...handleDefaults },
    tab: { ...tabDefaults },
  };
}

export const tweak = $state<TweakState>(initial());
export const status = $state({ error: "" });

/** What the panel receives: only edited tokens, so untouched ones keep following the source. */
export function payload(state: TweakState) {
  return {
    tokens: Object.fromEntries(
      Object.entries(state.tokens).filter(([name, value]) => value !== tokenDefaults[name]),
    ),
    motion: state.motion,
    handle: state.handle,
    tab: state.tab,
  };
}
export type Payload = ReturnType<typeof payload>;

export function editToken(name: string, value: string) {
  if (!valid(name, value)) {
    status.error = `Invalid CSS value for ${name}`;
    return;
  }
  tweak.tokens[name] = value;
  status.error = "";
}

/** Replaces the whole state with `preset` over the source values, keeping only valid entries. */
export function load(preset: unknown) {
  const next = initial();
  const { tokens, motion, handle, tab } = (
    preset && typeof preset === "object" ? preset : {}
  ) as Record<string, unknown>;
  if (tokens && typeof tokens === "object")
    for (const [name, value] of Object.entries(tokens))
      if (name in tokenDefaults && typeof value === "string" && valid(name, value))
        next.tokens[name] = value;
  if (motion && typeof motion === "object")
    for (const key of Object.keys(next.motion) as (keyof Motions)[]) {
      const value = (motion as Record<string, unknown>)[key];
      if (validMotion(value)) next.motion[key] = value;
    }
  if (validHandle(handle))
    next.handle = {
      activationPercent: handle.activationPercent,
      revealPercent: handle.revealPercent,
      rushSpeed: handle.rushSpeed,
    };
  // Presets saved before the tab section keep the release shape.
  if (validTab(tab))
    next.tab = { depth: tab.depth, height: tab.height, curve: tab.curve, wordmark: tab.wordmark };
  Object.assign(tweak, next);
}

// The working state survives the window, and the app restarts that a Rust change causes.
const working = "docdock-tweak-working-v1";
export function restore() {
  try {
    load(JSON.parse(localStorage.getItem(working) ?? "{}"));
  } catch {
    status.error = "Could not read the last tweaks.";
  }
}
export function remember(state: Payload) {
  try {
    localStorage.setItem(working, JSON.stringify(state));
  } catch {
    // Only a convenience: the presets and the export keep what matters.
  }
}
