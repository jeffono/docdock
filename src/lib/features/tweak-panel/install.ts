// Development builds only. In the panel: the button that opens the tweak window, and the receiver
// that applies the window's state. Nothing here holds the panel open or changes its behavior.
import { mount, unmount } from "svelte";
import {
  devFonts,
  devHandle,
  devMotion,
  onTweakFontsRequest,
  onTweakState,
  tweakFontsSend,
  tweakRequest,
} from "../../ipc";
import Launcher from "./Launcher.svelte";
import { receive } from "./receive";

let tokens: Record<string, string> = {};
let motion = "";
let handle = "";

function apply(event: unknown) {
  const state = receive(event);
  const style = document.documentElement.style;
  for (const name of Object.keys(tokens)) if (!(name in state.tokens)) style.removeProperty(name);
  for (const [name, value] of Object.entries(state.tokens)) style.setProperty(name, value);
  tokens = state.tokens;
  // Each command runs only when its section changed.
  const nextMotion = JSON.stringify(state.motion);
  if (nextMotion !== motion) {
    motion = nextMotion;
    devMotion(state.motion).catch((e) => console.error("dev_motion", e));
  }
  const nextHandle = JSON.stringify(state.handle);
  if (nextHandle !== handle) {
    handle = nextHandle;
    devHandle(state.handle).catch((e) => console.error("dev_handle", e));
  }
}

// The window cannot invoke `dev_fonts`, so the panel reads the list and sends it over.
function sendFonts() {
  devFonts()
    .then((families) => tweakFontsSend({ families }))
    .catch((e) => tweakFontsSend({ error: String(e) }))
    .catch((e) => console.error("tweak-fonts", e));
}

export function install(target: HTMLElement) {
  const launcher = mount(Launcher, { target });
  const listening = onTweakState(apply);
  const fonts = onTweakFontsRequest(sendFonts);
  // After a reload the panel starts from the source: the window, if open, answers with its state.
  void listening.then(() => tweakRequest());
  return () => {
    void unmount(launcher);
    void listening.then((unlisten) => unlisten());
    void fonts.then((unlisten) => unlisten());
  };
}
