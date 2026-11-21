// Floating windows from the panel's reading view: which element is a note image there, the badge
// laid in each image's corner to detach it or close its window, and how Rust learns where it sits.
import { floatingOpen } from "../../ipc";
import { PIP_FRAME, PIP_WINDOW } from "../../ui/Icon.svelte";
import "./badge.css";

/**
 * The local image `target` belongs to in a note being read: a grid thumbnail or a single image.
 * The editor lives in a shadow root, so its images never match.
 */
export function imageTarget(target: EventTarget | null): HTMLElement | null {
  if (!(target instanceof Element) || !target.closest(".reading .note-content")) return null;
  if (target.closest(".float-badge")) return null;
  return (
    target.closest<HTMLElement>(".image-thumb[data-image]") ??
    target.closest<HTMLElement>("img[data-image]")
  );
}

/**
 * `path`, relative to the notes folder, as Rust compares saved image paths: either separator,
 * empty and `.` parts dropped, `..` applied, case ignored. Empty if it leaves the folder.
 */
export function normalizePath(path: string): string {
  const parts: string[] = [];
  for (const part of path.split(/[/\\]/)) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (!parts.pop()) return "";
    } else parts.push(part);
  }
  return parts.join("/").toLowerCase();
}

const SVG = "http://www.w3.org/2000/svg";

function badge(): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "float-badge";
  button.tabIndex = -1;
  const icon = document.createElementNS(SVG, "svg");
  icon.setAttribute("viewBox", "0 0 24 24");
  icon.setAttribute("aria-hidden", "true");
  for (const [d, part] of [
    [PIP_FRAME, "pip-frame"],
    [PIP_WINDOW, "pip-window"],
  ]) {
    const path = document.createElementNS(SVG, "path");
    path.setAttribute("d", d);
    path.setAttribute("class", part);
    icon.append(path);
  }
  button.append(icon);
  return button;
}

/**
 * Lays a badge in the corner of every local image of a rendered note: each image goes into an
 * anchor beside its own badge, since a thumbnail is a button and an image holds nothing. Images
 * inside links are left alone. The badge scrolls with its image.
 */
export function decorate(body: HTMLElement) {
  for (const image of body.querySelectorAll<HTMLElement>(
    ".image-thumb[data-image], img[data-image]",
  )) {
    if (image instanceof HTMLImageElement && image.closest(".image-thumb")) continue;
    if (image.closest("a[href]") || image.parentElement?.classList.contains("float-anchor"))
      continue;
    const anchor = document.createElement("span");
    anchor.className = "float-anchor";
    image.replaceWith(anchor);
    anchor.append(image, badge());
  }
}

/** The image a badge belongs to. */
export function badgeImage(target: EventTarget | null): HTMLElement | null {
  if (!(target instanceof Element)) return null;
  const anchor = target.closest(".float-badge")?.parentElement;
  const image = anchor?.firstElementChild;
  return image instanceof HTMLElement && image.dataset.image ? image : null;
}

/**
 * Shows which images of `body` open floating windows show (`floating`, normalized paths): their
 * badges stay, filled, and close the window; the others show only on hover or focus.
 */
export function markFloating(
  body: HTMLElement,
  floating: ReadonlySet<string>,
  labels: { float: string; unfloat: string },
) {
  for (const button of body.querySelectorAll<HTMLElement>(".float-badge")) {
    const path = button.parentElement?.querySelector<HTMLElement>("[data-image]")?.dataset.image;
    const open = path !== undefined && floating.has(normalizePath(path));
    button.classList.toggle("floating", open);
    const label = open ? labels.unfloat : labels.float;
    button.setAttribute("aria-label", label);
    button.title = label;
  }
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
