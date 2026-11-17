// Sources are the user's drawings. Never rewrite them or resize a small drawing.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Resvg } from "@resvg/resvg-js";

const icons = join(import.meta.dirname, "../src-tauri/icons");

/** Renders `<prefix>-<size>.svg` 1:1 when drawn, otherwise `master` scaled to `size`. */
function render(prefix, size, master) {
  const drawing = join(icons, "source", `${prefix}-${size}.svg`);
  const exact = existsSync(drawing);
  const source = readFileSync(exact ? drawing : join(icons, "source", master));
  // An exact drawing has only a viewBox: its intrinsic size renders 1:1, with no fit transform.
  const image = new Resvg(source, {
    font: { loadSystemFonts: false },
    ...(exact ? {} : { fitTo: { mode: "width", value: size } }),
  }).render();
  if (image.width !== size || image.height !== size) {
    throw new Error(`Expected a ${size}x${size} drawing, got ${image.width}x${image.height}`);
  }
  return image.asPng();
}

const cache = new Map();
function png(size) {
  if (!cache.has(size)) {
    cache.set(size, render("icon", size, "icon-design-480.svg"));
  }
  return cache.get(size);
}

for (const [name, size] of [
  ["32x32.png", 32],
  ["128x128.png", 128],
  ["128x128@2x.png", 256],
]) {
  writeFileSync(join(icons, name), png(size));
}

// Windows Vista+ supports PNG payloads in ICO, preserving the rasterizer's RGBA pixels.
const sizes = [16, 20, 24, 30, 32, 36, 40, 48, 64, 256];
const directory = Buffer.alloc(6 + 16 * sizes.length);
directory.writeUInt16LE(1, 2); // Icon, not cursor.
directory.writeUInt16LE(sizes.length, 4);
let offset = directory.length;
for (const [index, size] of sizes.entries()) {
  const entry = 6 + 16 * index;
  directory[entry] = size === 256 ? 0 : size;
  directory[entry + 1] = size === 256 ? 0 : size;
  directory.writeUInt16LE(1, entry + 4); // One color plane.
  directory.writeUInt16LE(32, entry + 6); // RGBA, 8 bits per channel.
  directory.writeUInt32LE(png(size).length, entry + 8);
  directory.writeUInt32LE(offset, entry + 12);
  offset += png(size).length;
}
writeFileSync(join(icons, "icon.ico"), Buffer.concat([directory, ...sizes.map(png)]));

// Modern ICNS entries also carry PNGs; the length includes each eight-byte chunk header.
const chunks = [
  ["icp4", 16],
  ["icp5", 32],
  ["icp6", 64],
  ["ic07", 128],
  ["ic08", 256],
  ["ic09", 512],
  ["ic10", 1024],
].map(([type, size]) => {
  const header = Buffer.alloc(8);
  header.write(type);
  header.writeUInt32BE(8 + png(size).length, 4);
  return Buffer.concat([header, png(size)]);
});
const header = Buffer.alloc(8);
header.write("icns");
header.writeUInt32BE(8 + chunks.reduce((length, chunk) => length + chunk.length, 0), 4);
writeFileSync(join(icons, "icon.icns"), Buffer.concat([header, ...chunks]));

// Tray glyphs, embedded in the binary by `tray.rs`: "light" is black for a light taskbar, "dark" is
// white for a dark one. 40 and 64 have no drawing and come from the 48, the largest one.
mkdirSync(join(icons, "tray"), { recursive: true });
for (const theme of ["light", "dark"]) {
  for (const size of [16, 20, 24, 30, 32, 40, 48, 64]) {
    const prefix = `icon-tray-${theme}`;
    writeFileSync(
      join(icons, "tray", `${theme}-${size}.png`),
      render(prefix, size, `${prefix}-48.svg`),
    );
  }
}
console.log("Rebuilt the bundle PNGs, ICO, ICNS and tray PNGs from icons/source/.");
