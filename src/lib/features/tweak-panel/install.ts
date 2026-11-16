import { mount, unmount } from "svelte";
import TweakPanel from "./TweakPanel.svelte";

export function install(target: HTMLElement, hold: (open: boolean) => Promise<void>) {
  // Keep the preview visible, but prevent a note link from showing a native page over the tool.
  const content = [...target.children].filter(
    (node): node is HTMLElement => node instanceof HTMLElement,
  );
  const previous = content.map((node) => node.inert);
  let held = false;
  const restore = () =>
    content.forEach((node, i) => {
      node.inert = previous[i];
    });
  const panel = mount(TweakPanel, {
    target,
    props: {
      hold: async (open) => {
        await hold(open);
        held = open;
        if (open)
          content.forEach((node) => {
            node.inert = true;
          });
        else restore();
      },
    },
  });
  return () => {
    restore();
    if (held) void hold(false);
    void unmount(panel);
  };
}
