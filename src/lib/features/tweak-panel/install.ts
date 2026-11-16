// Development builds only. In the panel: the button that opens the tweak window, and the receiver
// that applies the window's state. Nothing here holds the panel open or changes its behavior.
import { mount, unmount } from "svelte";
import { reportHandleLook } from "../../handle-look";
import { devMotion, onTweakState, tweakRequest } from "../../ipc";
import Launcher from "./Launcher.svelte";
import type { Payload } from "./state.svelte";

let tokens: Record<string, string> = {};
let motion = "";

function apply(state: Payload) {
  const style = document.documentElement.style;
  for (const name of Object.keys(tokens)) if (!(name in state.tokens)) style.removeProperty(name);
  for (const [name, value] of Object.entries(state.tokens)) style.setProperty(name, value);
  tokens = state.tokens;
  // The edge handle is drawn natively from the colors.
  reportHandleLook();
  const next = JSON.stringify(state.motion);
  if (next === motion) return;
  motion = next;
  devMotion(state.motion).catch((e) => console.error("dev_motion", e));
}

export function install(target: HTMLElement) {
  const launcher = mount(Launcher, { target });
  const listening = onTweakState(apply);
  // After a reload the panel starts from the source: the window, if open, answers with its state.
  void listening.then(() => tweakRequest());
  return () => {
    void unmount(launcher);
    void listening.then((unlisten) => unlisten());
  };
}
