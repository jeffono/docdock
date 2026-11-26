export type TransformGroup = "case" | "clean" | "lines" | "format";

export type TransformId =
  | "uppercase"
  | "lowercase"
  | "sentence-case"
  | "capitalize-words"
  | "digits-only"
  | "alphanumeric"
  | "normalize-whitespace"
  | "strip-accents"
  | "remove-empty-lines"
  | "join-lines"
  | "sort-ascending"
  | "sort-descending"
  | "deduplicate-lines"
  | "clean-url"
  | "url-decode"
  | "format-json"
  | "minify-json";

export type PureTransform = {
  id: TransformId;
  group: TransformGroup;
  run: (text: string, locale: string) => string;
};

const TRACKING_PARAMS = new Set([
  "fbclid",
  "gclid",
  "gbraid",
  "wbraid",
  "dclid",
  "msclkid",
  "mc_cid",
  "mc_eid",
  "igshid",
  "si",
  "ref_src",
  "ref_url",
  "yclid",
  "mkt_tok",
  "_hsenc",
  "_hsmi",
  "oly_anon_id",
  "oly_enc_id",
  "vero_id",
  "spm",
  "s_cid",
  "icid",
  "zanpid",
  "tt_medium",
  "tt_content",
]);

function isTrackingParam(name: string): boolean {
  const lower = name.toLowerCase();
  return lower.startsWith("utm_") || TRACKING_PARAMS.has(lower);
}

export const upper = (text: string, locale: string) => text.toLocaleUpperCase(locale);
export const lower = (text: string, locale: string) => text.toLocaleLowerCase(locale);

function capitalize(text: string, locale: string) {
  return lower(text, locale).replace(/\p{L}/u, (letter) => upper(letter, locale));
}

export const sentenceCase = (text: string, locale: string): string => {
  // Sentence segmentation treats a lowercase word after a period as a continuation.
  // Use uppercase for boundary detection, keeping the source's UTF-16 offsets.
  const boundaries = text.replace(/\p{Ll}/gu, (letter) => {
    const raised = upper(letter, locale);
    return raised.length === letter.length ? raised : letter;
  });
  const sentences = [
    ...new Intl.Segmenter(locale, { granularity: "sentence" }).segment(boundaries),
  ];
  return sentences
    .map(({ index, segment }) => capitalize(text.slice(index, index + segment.length), locale))
    .join("");
};

export const capitalizeWords = (text: string, locale: string): string =>
  [...new Intl.Segmenter(locale, { granularity: "word" }).segment(text)]
    .map(({ segment, isWordLike }) => (isWordLike ? capitalize(segment, locale) : segment))
    .join("");

export const cycleCase = (text: string, locale: string): string => {
  const lowercase = lower(text, locale);
  const sentence = sentenceCase(text, locale);
  const words = capitalizeWords(text, locale);
  const uppercase = upper(text, locale);
  const forms = [lowercase, sentence, words, uppercase];
  // Skip equivalent forms (for example, sentence case and word case on one word).
  const current = forms.lastIndexOf(text);
  return forms.slice(current + 1).find((form) => form !== text) ?? lowercase;
};

export const digitsOnly = (text: string): string => text.replace(/[^0-9]+/g, "");

export const alphanumericOnly = (text: string): string => text.replace(/[^\p{L}\p{N}]+/gu, "");

export const normalizeWhitespace = (text: string): string =>
  text
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.replace(/[ \t\f\v]+/g, " ").trim())
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();

export const stripAccents = (text: string): string =>
  text.normalize("NFD").replace(/\p{M}/gu, "").normalize("NFC");

export const removeEmptyLines = (text: string): string =>
  text
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .join("\n");

export const joinLines = (text: string): string =>
  text
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .join(" ");

export function sortLines(text: string, locale: string, direction: 1 | -1): string {
  const { compare } = new Intl.Collator(locale, { numeric: true, sensitivity: "base" });
  return text
    .split("\n")
    .sort((a, b) => direction * compare(a, b))
    .join("\n");
}

export function deduplicateLines(text: string): string {
  return [...new Set(text.split("\n"))].join("\n");
}

function splitTrailingUrlPunctuation(raw: string): [string, string] {
  let end = raw.length;
  while (end > 0) {
    const ch = raw[end - 1];
    if (",.;:!?".includes(ch)) {
      end--;
      continue;
    }
    if (ch === ")") {
      const slice = raw.slice(0, end);
      const opens = (slice.match(/\(/g) ?? []).length;
      const closes = (slice.match(/\)/g) ?? []).length;
      if (closes > opens) {
        end--;
        continue;
      }
    }
    if (ch === "]") {
      const slice = raw.slice(0, end);
      const opens = (slice.match(/\[/g) ?? []).length;
      const closes = (slice.match(/\]/g) ?? []).length;
      if (closes > opens) {
        end--;
        continue;
      }
    }
    break;
  }
  return [raw.slice(0, end), raw.slice(end)];
}

function decodeQueryName(rawName: string): string {
  try {
    return decodeURIComponent(rawName.replace(/\+/g, " "));
  } catch {
    return rawName;
  }
}

export const cleanUrl = (text: string): string =>
  text.replace(/https?:\/\/[^\s<>"'`]+/gi, (match) => {
    const [candidate, trailing] = splitTrailingUrlPunctuation(match);
    try {
      new URL(candidate);
    } catch {
      return match;
    }
    const hashIdx = candidate.indexOf("#");
    const withoutHash = hashIdx < 0 ? candidate : candidate.slice(0, hashIdx);
    const hash = hashIdx < 0 ? "" : candidate.slice(hashIdx);
    const qIdx = withoutHash.indexOf("?");
    if (qIdx < 0) return match;
    const base = withoutHash.slice(0, qIdx);
    const rawQuery = withoutHash.slice(qIdx + 1);
    if (!rawQuery) return match;

    const kept: string[] = [];
    let removed = false;
    for (const pair of rawQuery.split("&")) {
      if (!pair) continue;
      const eqIdx = pair.indexOf("=");
      const rawName = eqIdx < 0 ? pair : pair.slice(0, eqIdx);
      if (isTrackingParam(decodeQueryName(rawName))) {
        removed = true;
      } else {
        kept.push(pair);
      }
    }
    if (!removed) return match;
    const queryPart = kept.length ? `?${kept.join("&")}` : "";
    return `${base}${queryPart}${hash}${trailing}`;
  });

export const urlDecode = (text: string): string =>
  text.replace(/(?:%[0-9A-Fa-f]{2})+/g, (encoded) => {
    try {
      return decodeURIComponent(encoded);
    } catch {
      return encoded;
    }
  });

export function isStructuredJson(text: string): boolean {
  const trimmed = text.trim();
  if (
    !(
      (trimmed.startsWith("{") && trimmed.endsWith("}")) ||
      (trimmed.startsWith("[") && trimmed.endsWith("]"))
    )
  ) {
    return false;
  }
  try {
    JSON.parse(trimmed);
    return true;
  } catch {
    return false;
  }
}

function tokenizeJson(trimmed: string): string[] {
  const tokens: string[] = [];
  let i = 0;
  const len = trimmed.length;
  while (i < len) {
    const ch = trimmed[i];
    if (ch === " " || ch === "\t" || ch === "\n" || ch === "\r") {
      i++;
      continue;
    }
    if ("{}[]:,".includes(ch)) {
      tokens.push(ch);
      i++;
      continue;
    }
    if (ch === '"') {
      const start = i;
      i++;
      while (i < len) {
        const c = trimmed[i];
        if (c === "\\") {
          i += 2;
          continue;
        }
        if (c === '"') {
          i++;
          break;
        }
        i++;
      }
      tokens.push(trimmed.slice(start, i));
      continue;
    }
    const start = i;
    while (i < len) {
      const c = trimmed[i];
      if (c === " " || c === "\t" || c === "\n" || c === "\r" || "{}[]:,".includes(c)) break;
      i++;
    }
    tokens.push(trimmed.slice(start, i));
  }
  return tokens;
}

/**
 * Formats or minifies valid JSON objects/arrays while preserving exact numeric representations
 * (such as >2^53 integers and trailing decimals) and duplicate object keys verbatim.
 */
function reformatJson(text: string, indent: number | null): string {
  if (!isStructuredJson(text)) return text;
  const tokens = tokenizeJson(text.trim());
  if (indent === null) return tokens.join("");

  const out: string[] = [];
  let depth = 0;
  for (let i = 0; i < tokens.length; i++) {
    const tok = tokens[i];
    if (tok === "{" || tok === "[") {
      const close = tok === "{" ? "}" : "]";
      if (tokens[i + 1] === close) {
        out.push(`${tok}${close}`);
        i++;
      } else {
        depth++;
        out.push(`${tok}\n${" ".repeat(depth * indent)}`);
      }
    } else if (tok === "}" || tok === "]") {
      depth--;
      out.push(`\n${" ".repeat(depth * indent)}${tok}`);
    } else if (tok === ",") {
      out.push(`,\n${" ".repeat(depth * indent)}`);
    } else if (tok === ":") {
      out.push(": ");
    } else {
      out.push(tok);
    }
  }
  return out.join("");
}

export const formatJson = (text: string): string => reformatJson(text, 2);

export const minifyJson = (text: string): string => reformatJson(text, null);

export const pureTransforms: readonly PureTransform[] = [
  { id: "uppercase", group: "case", run: upper },
  { id: "lowercase", group: "case", run: lower },
  { id: "sentence-case", group: "case", run: sentenceCase },
  { id: "capitalize-words", group: "case", run: capitalizeWords },
  { id: "digits-only", group: "clean", run: digitsOnly },
  { id: "alphanumeric", group: "clean", run: alphanumericOnly },
  { id: "normalize-whitespace", group: "clean", run: normalizeWhitespace },
  { id: "strip-accents", group: "clean", run: stripAccents },
  { id: "remove-empty-lines", group: "lines", run: removeEmptyLines },
  { id: "join-lines", group: "lines", run: joinLines },
  { id: "sort-ascending", group: "lines", run: (text, locale) => sortLines(text, locale, 1) },
  { id: "sort-descending", group: "lines", run: (text, locale) => sortLines(text, locale, -1) },
  { id: "deduplicate-lines", group: "lines", run: deduplicateLines },
  { id: "clean-url", group: "format", run: cleanUrl },
  { id: "url-decode", group: "format", run: urlDecode },
  { id: "format-json", group: "format", run: formatJson },
  { id: "minify-json", group: "format", run: minifyJson },
];

const byId = new Map(pureTransforms.map((item) => [item.id, item]));

export function applyTransformChain(
  source: string,
  ids: readonly TransformId[],
  locale: string,
): string {
  let current = source;
  for (const id of ids) {
    const transform = byId.get(id);
    if (transform) current = transform.run(current, locale);
  }
  return current;
}

export function suggestTransforms(
  text: string,
  locale = "en",
  limit = 4,
): readonly PureTransform[] {
  const candidates: TransformId[] = [];
  if (isStructuredJson(text)) {
    candidates.push("format-json", "minify-json");
  }
  if (/https?:\/\//i.test(text)) {
    candidates.push("clean-url", "url-decode");
  } else if (/%[0-9A-Fa-f]{2}/.test(text)) {
    candidates.push("url-decode");
  }
  const trimmed = text.trim();
  if (!trimmed.includes("\n") && trimmed.length <= 64) {
    const digits = (trimmed.match(/\d/g) ?? []).length;
    if (digits >= 4 && /\D/.test(trimmed)) candidates.push("digits-only");
    if (/[^\p{L}\p{N}\s]/u.test(trimmed) && !/\s/.test(trimmed)) candidates.push("alphanumeric");
  }
  if (text.includes("\n")) {
    candidates.push("remove-empty-lines", "deduplicate-lines", "join-lines", "sort-ascending");
  }
  candidates.push(
    "normalize-whitespace",
    "strip-accents",
    "lowercase",
    "uppercase",
    "sentence-case",
    "capitalize-words",
  );

  const chosen: PureTransform[] = [];
  const seen = new Set<TransformId>();
  for (const id of candidates) {
    if (seen.has(id)) continue;
    seen.add(id);
    const transform = byId.get(id);
    if (!transform || transform.run(text, locale) === text) continue;
    chosen.push(transform);
    if (chosen.length >= limit) break;
  }
  return chosen;
}
