// Detaching a note image into a floating window, from the panel's reading view: which element is
// an image there, and how Rust learns where it sits.
import { floatingOpen } from "../../ipc";

/**
 * The local image `target` belongs to in a note being read: a grid thumbnail or a single image.
 * The editor lives in a shadow root, so its images never match.
 */
export function imageTarget(target: EventTarget | null): HTMLElement | null {
  if (!(target instanceof Element) || !target.closest(".reading .note-content")) return null;
  return (
    target.closest<HTMLElement>(".image-thumb[data-image]") ??
    target.closest<HTMLElement>("img[data-image]")
  );
}

/** Opens `target`'s image as a floating window, where the side preview would show it. */
export function openFloating(target: HTMLElement): Promise<void> {
  const path = target.dataset.image;
  if (!path) return Promise.resolve();
  const image = target instanceof HTMLImageElement ? target : target.querySelector("img");
  const box = target.getBoundingClientRect();
  const rect: [number, number, number, number] = [
    Math.round(box.left),
    Math.round(box.top),
    Math.round(box.width),
    Math.round(box.height),
  ];
  // Rust sizes the window from the image; one not loaded yet opens at a default size.
  const natural: [number, number] = image?.complete
    ? [image.naturalWidth, image.naturalHeight]
    : [0, 0];
  return floatingOpen(path, rect, natural);
}
