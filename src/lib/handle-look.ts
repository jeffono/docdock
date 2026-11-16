import { type HandleLook, panelHandleLook } from "./ipc";

/** A computed sRGB color as its three channels. */
function channels(color: string): [number, number, number] {
  const [red = 0, green = 0, blue = 0] = (color.match(/[\d.]+/g) ?? []).map(Number);
  return [red, green, blue].map(Math.round) as [number, number, number];
}

/** Sends the edge handle the current theme's tokens; call again whenever they change. */
export function reportHandleLook() {
  const probe = document.createElement("span");
  probe.style.color = "var(--color-fg)";
  probe.style.backgroundColor = "var(--color-bg)";
  probe.style.borderColor = "var(--color-line)";
  probe.style.outlineColor = "var(--color-accent)";
  document.body.append(probe);
  const style = getComputedStyle(probe);
  const text = channels(style.color);
  probe.style.color = "var(--color-muted)";
  const muted = channels(getComputedStyle(probe).color);
  const look: HandleLook = {
    background: channels(style.backgroundColor),
    line: channels(style.borderTopColor),
    accent: channels(style.outlineColor),
    text,
    muted,
  };
  probe.remove();
  // Without it the handle stays hidden, and the edge stretch still opens the panel.
  void panelHandleLook(look).catch(() => {});
}
