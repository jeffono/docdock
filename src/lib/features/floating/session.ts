// A floating window's first state, asked once before choosing what to mount (an image or a
// post-it), and the updates Rust sends after it, held until the mounted window listens.
import { type FloatUpdate, floatState, type Locale } from "../../ipc";

export type FloatSession = {
  state: FloatUpdate & { locale: Locale };
  /** Hands `handler` the updates that arrived meanwhile, then every later one. */
  listen: (handler: (update: FloatUpdate) => void) => void;
};

export async function floatSession(): Promise<FloatSession> {
  let handler: ((update: FloatUpdate) => void) | null = null;
  const queued: FloatUpdate[] = [];
  const state = await floatState((update) => {
    if (handler) handler(update);
    else queued.push(update);
  });
  return {
    state,
    listen(next) {
      handler = next;
      for (const update of queued.splice(0)) next(update);
    },
  };
}
