import { describe, expect, it } from "vitest";
import {
  activeTransformCount,
  alphanumericOnly,
  applyTransformState,
  classifyTransformControls,
  cleanUrl,
  countActiveInControls,
  digitsOnly,
  EMPTY_TRANSFORM_STATE,
  formatJson,
  isStructuredJson,
  isTransformActive,
  joinLines,
  minifyJson,
  normalizeWhitespace,
  pureTransforms,
  removeEmptyLines,
  stripAccents,
  type TransformId,
  toggleTransform,
  urlDecode,
} from "./text-transforms";

describe("shared text transforms", () => {
  it("extracts digits only from formatted numbers", () => {
    expect(digitsOnly("+55 (11) 98765-4321")).toBe("5511987654321");
  });

  it("strips punctuation and whitespace while preserving accented letters and digits", () => {
    expect(alphanumericOnly("Ação #2026: R$ 10,00!")).toBe("Ação2026R1000");
  });

  it("normalizes CRLF, collapses repeated spaces, and trims excess blank lines", () => {
    expect(normalizeWhitespace("  one\t  two  \r\n\r\n\r\n\r\nthree   four \r\n")).toBe(
      "one two\n\nthree four",
    );
  });

  it("strips combining accents while preserving base characters", () => {
    expect(stripAccents("Ação, coração e Übung")).toBe("Acao, coracao e Ubung");
  });

  it("removes blank and whitespace-only lines", () => {
    expect(removeEmptyLines("alpha\r\n   \r\nbeta\n\ngamma")).toBe("alpha\nbeta\ngamma");
  });

  it("joins trimmed non-empty lines with single spaces", () => {
    expect(joinLines("  first \n\n  second\n  third  ")).toBe("first second third");
  });

  it("removes tracking parameters from URLs while keeping functional query params and hash", () => {
    const input = "See (https://example.com/path?utm_source=x&q=docdock&fbclid=abc#section).";
    expect(cleanUrl(input)).toBe("See (https://example.com/path?q=docdock#section).");
  });

  it("leaves clean URLs byte-identical without modifying trailing slashes", () => {
    expect(cleanUrl("https://example.com?q=rust")).toBe("https://example.com?q=rust");
  });

  it("preserves IDN hosts, %20 spacing, raw colons/slashes, and hash question marks when cleaning", () => {
    expect(
      cleanUrl(
        "https://café.example?a=1%202&callback=https://x/y:z&utm_source=mail#frag?utm_medium=1",
      ),
    ).toBe("https://café.example?a=1%202&callback=https://x/y:z#frag?utm_medium=1");
  });

  it("decodes valid UTF-8 percent sequences without failing on malformed percents", () => {
    expect(urlDecode("https://example.com/?q=ol%C3%A1%20mundo&raw=100%done%FF")).toBe(
      "https://example.com/?q=olá mundo&raw=100%done%FF",
    );
  });

  it("pretty-prints valid JSON objects and arrays while preserving integers, trailing zeros, and duplicate keys", () => {
    expect(formatJson(' {"b":1,"a":[true]} ')).toBe('{\n  "b": 1,\n  "a": [\n    true\n  ]\n}');
    expect(
      formatJson('{"id":9007199254740993,"price":1.0,"huge":1e400,"k":1,"k":2,"empty":{}}'),
    ).toBe(
      '{\n  "id": 9007199254740993,\n  "price": 1.0,\n  "huge": 1e400,\n  "k": 1,\n  "k": 2,\n  "empty": {}\n}',
    );
    expect(formatJson("42")).toBe("42");
    expect(isStructuredJson('{"ok":true}')).toBe(true);
    expect(isStructuredJson("12345")).toBe(false);
  });

  it("minifies formatted JSON cleanly without corrupting numbers", () => {
    expect(minifyJson('{\n  "id": 9007199254740993,\n  "ratio": 1.0,\n  "b": [2, 3]\n}')).toBe(
      '{"id":9007199254740993,"ratio":1.0,"b":[2,3]}',
    );
  });

  it("replaces mutually exclusive siblings and toggles off active controls cleanly", () => {
    let state = toggleTransform(EMPTY_TRANSFORM_STATE, "lowercase");
    expect(state.case).toBe("lowercase");
    expect(isTransformActive(state, "lowercase")).toBe(true);

    state = toggleTransform(state, "uppercase");
    expect(state.case).toBe("uppercase");
    expect(isTransformActive(state, "lowercase")).toBe(false);
    expect(activeTransformCount(state)).toBe(1);

    state = toggleTransform(state, "uppercase");
    expect(state).toEqual(EMPTY_TRANSFORM_STATE);
    expect(activeTransformCount(state)).toBe(0);

    for (const [first, second] of [
      ["format-json", "minify-json"],
      ["remove-empty-lines", "join-lines"],
      ["sort-ascending", "sort-descending"],
      ["digits-only", "alphanumeric"],
    ] as const) {
      let s = toggleTransform(EMPTY_TRANSFORM_STATE, first);
      expect(isTransformActive(s, first)).toBe(true);
      s = toggleTransform(s, second);
      expect(isTransformActive(s, first)).toBe(false);
      expect(isTransformActive(s, second)).toBe(true);
      s = toggleTransform(s, second);
      expect(activeTransformCount(s)).toBe(0);
    }

    state = toggleTransform(EMPTY_TRANSFORM_STATE, "strip-accents");
    state = toggleTransform(state, "deduplicate-lines");
    expect(state.toggles).toEqual(["strip-accents", "deduplicate-lines"]);
    state = toggleTransform(state, "strip-accents");
    expect(state.toggles).toEqual(["deduplicate-lines"]);
  });

  it("applies transforms in deterministic canonical order regardless of toggle order", () => {
    const orderA = toggleTransform(
      toggleTransform(toggleTransform(EMPTY_TRANSFORM_STATE, "join-lines"), "sort-ascending"),
      "deduplicate-lines",
    );
    const orderB = toggleTransform(
      toggleTransform(
        toggleTransform(EMPTY_TRANSFORM_STATE, "sort-ascending"),
        "deduplicate-lines",
      ),
      "join-lines",
    );
    const multiline = "zeta\nalpha\nzeta\nbeta";
    expect(applyTransformState(multiline, orderA, "en")).toBe("alpha beta zeta");
    expect(applyTransformState(multiline, orderB, "en")).toBe("alpha beta zeta");

    const urlToJson = toggleTransform(
      toggleTransform(toggleTransform(EMPTY_TRANSFORM_STATE, "format-json"), "url-decode"),
      "clean-url",
    );
    expect(applyTransformState('{"id":1,"utm":"%20"}', urlToJson, "en")).toBe(
      '{\n  "id": 1,\n  "utm": " "\n}',
    );
    expect(
      applyTransformState(
        "https://example.com/?utm_source=x&q=ol%C3%A1%20mundo",
        toggleTransform(toggleTransform(EMPTY_TRANSFORM_STATE, "url-decode"), "clean-url"),
        "en",
      ),
    ).toBe("https://example.com/?q=olá mundo");

    const camel = toggleTransform(
      toggleTransform(toggleTransform(EMPTY_TRANSFORM_STATE, "alphanumeric"), "capitalize-words"),
      "strip-accents",
    );
    expect(applyTransformState("ação rápida 12", camel, "en")).toBe("AcaoRapida12");

    const caseDedup = toggleTransform(
      toggleTransform(EMPTY_TRANSFORM_STATE, "deduplicate-lines"),
      "lowercase",
    );
    expect(applyTransformState("Hello\nhello", caseDedup, "en")).toBe("hello");
  });

  it("partitions all 17 transforms exactly once per item without duplicates", () => {
    const collectIds = (text: string) => {
      const layout = classifyTransformControls(text);
      const ids: TransformId[] = [];
      for (const item of [...layout.primary, ...layout.secondary]) {
        if (item.kind === "segmented") ids.push(...item.options);
        else ids.push(item.id);
      }
      return { layout, ids };
    };

    for (const sample of [
      '{"a":1}',
      "https://example.com/?utm_source=mail&id=9%200",
      "  Café\n\ncafé  ",
      "(11) 91234-5678",
      "hello world",
    ]) {
      const { ids } = collectIds(sample);
      expect(ids).toHaveLength(pureTransforms.length);
      expect(new Set(ids)).toEqual(new Set(pureTransforms.map((t) => t.id)));
    }

    const jsonLayout = classifyTransformControls('{"a":1}');
    expect(jsonLayout.primary).toEqual([
      { kind: "segmented", slot: "json", options: ["format-json", "minify-json"] },
    ]);

    const urlLayout = classifyTransformControls("https://example.com/?utm_source=mail&id=9");
    expect(urlLayout.primary).toEqual([
      { kind: "toggle", id: "clean-url" },
      { kind: "toggle", id: "url-decode" },
    ]);

    const codeLayout = classifyTransformControls("AB-123/45");
    expect(codeLayout.primary.some((c) => c.kind === "segmented" && c.slot === "extract")).toBe(
      true,
    );

    const state = toggleTransform(EMPTY_TRANSFORM_STATE, "format-json");
    expect(countActiveInControls(state, urlLayout.secondary)).toBe(1);
  });
});
