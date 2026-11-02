import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const root = join(import.meta.dirname, "..", "src");
const files = [];
function walk(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) walk(path);
    else if (/\.(?:svelte|css|ts|js)$/.test(entry.name) && entry.name !== "tokens.css")
      files.push(path);
  }
}
function withoutComments(text) {
  return text.replace(
    /("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|\/\*[\s\S]*?\*\/|\/\/[^\r\n]*/g,
    (part, quoted) => quoted ?? " ".repeat(part.length),
  );
}

walk(root);
let failed = false;
for (const file of files) {
  const lines = withoutComments(readFileSync(file, "utf8")).split(/\r?\n/);
  lines.forEach((line, index) => {
    const color =
      /(?:^|[;{])\s*[a-z-]+\s*:\s*[^;{}]*#[\da-f]{3,8}\b|\b(?:rgb|rgba|hsl|hsla|oklch|oklab|lab|lch|hwb|color-mix)\s*\(/i;
    if (color.test(line)) {
      console.error(`${relative(process.cwd(), file)}:${index + 1}: ${line.trim()}`);
      failed = true;
    }
  });
}
if (failed) process.exitCode = 1;
