import type { PageSelection } from "../../ipc";
import { pageHost } from "./list";
import { quoteYaml } from "./markdown";

/** Every line stays inside the quote, including blank lines and Markdown delimiters. */
export function blockquote(text: string): string {
  return text
    ? text
        .replace(/\r\n?/g, "\n")
        .split("\n")
        .map((line) => (line ? `> ${line}` : ">"))
        .join("\n")
    : "";
}

export function pageNote(page: PageSelection): { name: string; text: string } {
  const sanitized = Array.from(page.title, (c) => (c < " " || '<>:"/\\|?*'.includes(c) ? " " : c))
    .join("")
    .replace(/^[.\s]+|[.\s]+$/g, "");
  return {
    name: sanitized || pageHost(page.url),
    text: `---\ntitle: ${quoteYaml(page.title)}\nurl: ${quoteYaml(page.url)}\n---\n\n${blockquote(page.selection)}`,
  };
}
