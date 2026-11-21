// The preview window's image changes only once the next one is decoded, so switching thumbnails
// never flashes an empty frame, and any image no longer wanted, decoded or not, lets go of its
// note image at once: none stays in memory once the preview hides (decision 0009).

/** The part of `HTMLImageElement` the swap uses. */
export type Decodable = { src: string; decode(): Promise<void> };

/** Returns `show`: a URL to decode and then `mount`, or `null` to `unmount` what is shown. */
export function imageSwap<T extends Decodable>(
  create: () => T,
  mount: (image: T) => void,
  unmount: () => void,
) {
  let pending: T | null = null;
  return async function show(url: string | null) {
    // An empty `src` stops its load; its decode then fails or is ignored.
    if (pending) pending.src = "";
    pending = null;
    if (url === null) {
      unmount();
      return;
    }
    const image = create();
    pending = image;
    image.src = url;
    try {
      await image.decode();
    } catch {
      if (pending === image) {
        pending = null;
        unmount();
      }
      return;
    }
    if (pending !== image) return;
    pending = null;
    mount(image);
  };
}
