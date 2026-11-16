// Sources are the user's drawings. Never rewrite them or resize a small drawing.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Resvg } from "@resvg/resvg-js";

const icons = join(import.meta.dirname, "../src-tauri/icons");
const cache = new Map();
function png(size) {
  if (!cache.has(size)) {
    const drawing = join(icons, "source", `icon-${size}.svg`);
    const exact = existsSync(drawing);
    const source = readFileSync(exact ? drawing : join(icons, "source/icon-design-480.svg"));
    // An exact drawing has only a viewBox: its intrinsic size renders 1:1, with no fit transform.
    const image = new Resvg(source, {
      font: { loadSystemFonts: false },
      ...(exact ? {} : { fitTo: { mode: "width", value: size } }),
    }).render();
    if (image.width !== size || image.height !== size) {
      throw new Error(`Expected a ${size}x${size} drawing, got ${image.width}x${image.height}`);
    }
    cache.set(size, image.asPng());
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
console.log("Rebuilt the bundle PNGs, ICO and ICNS from icons/source/.");
