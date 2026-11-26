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

export type CaseTransformId = "lowercase" | "sentence-case" | "capitalize-words" | "uppercase";
export type JsonTransformId = "format-json" | "minify-json";
export type LinesTransformId = "remove-empty-lines" | "join-lines";
export type SortTransformId = "sort-ascending" | "sort-descending";
export type ExtractTransformId = "digits-only" | "alphanumeric";
export type ExclusiveSlot = "case" | "json" | "lines" | "sort" | "extract";

export type ToggleTransformId =
  | "clean-url"
  | "url-decode"
  | "normalize-whitespace"
  | "strip-accents"
  | "deduplicate-lines";

export type TransformState = {
  case?: CaseTransformId;
  json?: JsonTransformId;
  lines?: LinesTransformId;
  sort?: SortTransformId;
  extract?: ExtractTransformId;
  toggles: readonly ToggleTransformId[];
};

export const EMPTY_TRANSFORM_STATE: TransformState = { toggles: [] };

export type TransformControlItem =
  | { kind: "segmented"; slot: ExclusiveSlot; options: readonly TransformId[] }
  | { kind: "toggle"; id: ToggleTransformId };

export type TransformLayout = {
  primary: readonly TransformControlItem[];
  secondary: readonly TransformControlItem[];
};

export const exclusiveGroups: Record<ExclusiveSlot, readonly TransformId[]> = {
  json: ["format-json", "minify-json"],
  lines: ["remove-empty-lines", "join-lines"],
  sort: ["sort-ascending", "sort-descending"],
  case: ["lowercase", "sentence-case", "capitalize-words", "uppercase"],
  extract: ["digits-only", "alphanumeric"],
};

export const independentToggles: readonly ToggleTransformId[] = [
  "clean-url",
  "url-decode",
  "normalize-whitespace",
  "strip-accents",
  "deduplicate-lines",
];

const slotByTransform: Partial<Record<TransformId, ExclusiveSlot>> = {
  lowercase: "case",
  "sentence-case": "case",
  "capitalize-words": "case",
  uppercase: "case",
  "format-json": "json",
  "minify-json": "json",
  "remove-empty-lines": "lines",
  "join-lines": "lines",
  "sort-ascending": "sort",
  "sort-descending": "sort",
  "digits-only": "extract",
  alphanumeric: "extract",
};

const byId = new Map(pureTransforms.map((item) => [item.id, item]));

export function toggleTransform(state: TransformState, id: TransformId): TransformState {
  const slot = slotByTransform[id];
  if (!slot) {
    const toggle = id as ToggleTransformId;
    const exists = state.toggles.includes(toggle);
    return {
      ...state,
      toggles: exists
        ? state.toggles.filter((item) => item !== toggle)
        : [...state.toggles, toggle],
    };
  }
  switch (slot) {
    case "case":
      return { ...state, case: state.case === id ? undefined : (id as CaseTransformId) };
    case "json":
      return { ...state, json: state.json === id ? undefined : (id as JsonTransformId) };
    case "lines":
      return { ...state, lines: state.lines === id ? undefined : (id as LinesTransformId) };
    case "sort":
      return { ...state, sort: state.sort === id ? undefined : (id as SortTransformId) };
    case "extract":
      return { ...state, extract: state.extract === id ? undefined : (id as ExtractTransformId) };
  }
}

export function isTransformActive(state: TransformState, id: TransformId): boolean {
  const slot = slotByTransform[id];
  return slot ? state[slot] === id : state.toggles.includes(id as ToggleTransformId);
}

export function activeTransformCount(state: TransformState): number {
  let count = state.toggles.length;
  if (state.case) count++;
  if (state.json) count++;
  if (state.lines) count++;
  if (state.sort) count++;
  if (state.extract) count++;
  return count;
}

export function countActiveInControls(
  state: TransformState,
  controls: readonly TransformControlItem[],
): number {
  let count = 0;
  for (const control of controls) {
    if (control.kind === "segmented") {
      if (state[control.slot] !== undefined) count++;
    } else if (state.toggles.includes(control.id)) {
      count++;
    }
  }
  return count;
}

/** Applies active transforms in fixed canonical order regardless of click order. */
export function applyTransformState(source: string, state: TransformState, locale: string): string {
  const ordered: TransformId[] = [];
  const activeToggles = new Set(state.toggles);
  if (activeToggles.has("clean-url")) ordered.push("clean-url");
  if (activeToggles.has("url-decode")) ordered.push("url-decode");
  if (state.json) ordered.push(state.json);
  if (activeToggles.has("normalize-whitespace")) ordered.push("normalize-whitespace");
  if (activeToggles.has("strip-accents")) ordered.push("strip-accents");
  if (state.case) ordered.push(state.case);
  if (state.lines === "remove-empty-lines") ordered.push("remove-empty-lines");
  if (activeToggles.has("deduplicate-lines")) ordered.push("deduplicate-lines");
  if (state.sort) ordered.push(state.sort);
  if (state.lines === "join-lines") ordered.push("join-lines");
  if (state.extract) ordered.push(state.extract);

  let current = source;
  for (const id of ordered) {
    const transform = byId.get(id);
    if (transform) current = transform.run(current, locale);
  }
  return current;
}

/**
 * Partitions all 17 transforms into primary and secondary controls from the immutable original
 * text so controls never jump or disappear while toggling options.
 */
export function classifyTransformControls(originalText: string): TransformLayout {
  const hasJson = isStructuredJson(originalText);
  const hasUrl = /https?:\/\//i.test(originalText);
  const hasPercent = /%[0-9A-Fa-f]{2}/.test(originalText);
  const hasLines = originalText.includes("\n");
  const hasAccents = /\p{M}/u.test(originalText.normalize("NFD"));
  const hasMessySpaces = normalizeWhitespace(originalText) !== originalText;
  const trimmed = originalText.trim();
  const hasLetters = /\p{L}/u.test(originalText);
  const isShortCode =
    !hasLines &&
    !hasUrl &&
    !hasJson &&
    trimmed.length <= 64 &&
    (((trimmed.match(/\d/g) ?? []).length >= 4 && /\D/.test(trimmed)) ||
      (/[^\p{L}\p{N}\s]/u.test(trimmed) && !/\s/.test(trimmed)));

  const primary: TransformControlItem[] = [];
  const secondary: TransformControlItem[] = [];

  const pushSegmented = (slot: ExclusiveSlot, inPrimary: boolean) => {
    const item: TransformControlItem = { kind: "segmented", slot, options: exclusiveGroups[slot] };
    (inPrimary ? primary : secondary).push(item);
  };
  const pushToggle = (id: ToggleTransformId, inPrimary: boolean) => {
    const item: TransformControlItem = { kind: "toggle", id };
    (inPrimary ? primary : secondary).push(item);
  };

  pushSegmented("json", hasJson);
  pushToggle("clean-url", hasUrl);
  pushToggle("url-decode", hasUrl || hasPercent);
  pushSegmented("case", !hasJson && !hasUrl && (hasLetters || (!hasLines && !isShortCode)));
  pushSegmented("lines", hasLines && !hasJson);
  pushSegmented("sort", hasLines && !hasJson);
  pushToggle("deduplicate-lines", hasLines && !hasJson);
  pushSegmented("extract", isShortCode);
  pushToggle("normalize-whitespace", !hasJson && !hasUrl && hasMessySpaces);
  pushToggle("strip-accents", !hasJson && !hasUrl && hasAccents);

  return { primary, secondary };
}
