// The panel trusts nothing in a `tweak-state` event: only known tokens with valid values, and
// valid motions and handle tuning, reach the page and the native side.
import { type HandleTuning, handleDefaults, validHandle } from "./handle";
import { type Motions, motionDefaults, validMotion } from "./motion";
import { type TabTuning, tabDefaults, validTab } from "./tab-shape";
import { defaults as tokenDefaults, valid } from "./tokens";

export type Received = {
  tokens: Record<string, string>;
  motion: Motions;
  handle: HandleTuning;
  tab: TabTuning;
};

export function receive(event: unknown): Received {
  const { tokens, motion, handle, tab } = (
    event && typeof event === "object" ? event : {}
  ) as Record<string, unknown>;
  const received: Received = {
    tokens: {},
    motion: structuredClone(motionDefaults),
    handle: { ...handleDefaults },
    tab: { ...tabDefaults },
  };
  if (tokens && typeof tokens === "object")
    for (const [name, value] of Object.entries(tokens))
      if (name in tokenDefaults && typeof value === "string" && valid(name, value))
        received.tokens[name] = value;
  if (motion && typeof motion === "object")
    for (const key of Object.keys(received.motion) as (keyof Motions)[]) {
      const value = (motion as Record<string, unknown>)[key];
      if (validMotion(value)) received.motion[key] = value;
    }
  if (validHandle(handle))
    received.handle = {
      activationPercent: handle.activationPercent,
      revealPercent: handle.revealPercent,
      rushSpeed: handle.rushSpeed,
    };
  if (validTab(tab))
    received.tab = {
      depth: tab.depth,
      height: tab.height,
      curve: tab.curve,
      wordmark: tab.wordmark,
    };
  return received;
}
