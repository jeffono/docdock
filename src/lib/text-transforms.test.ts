import { describe, expect, it } from "vitest";
import {
  alphanumericOnly,
  applyTransformChain,
  cleanUrl,
  digitsOnly,
  formatJson,
  isStructuredJson,
  joinLines,
  minifyJson,
  normalizeWhitespace,
  removeEmptyLines,
  stripAccents,
  suggestTransforms,
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

  it("chains multiple transforms sequentially", () => {
    expect(
      applyTransformChain("  Café — 12  ", ["strip-accents", "alphanumeric", "lowercase"], "en"),
    ).toBe("cafe12");
  });

  it("prioritizes contextual suggestions and avoids alphanumeric on plain spaced phrases", () => {
    expect(suggestTransforms('{"a":1}', "en").map((t) => t.id)).toContain("format-json");
    expect(suggestTransforms("https://example.com/?utm_source=mail&id=9", "en")[0].id).toBe(
      "clean-url",
    );
    expect(suggestTransforms("(11) 91234-5678", "en").map((t) => t.id)).toContain("digits-only");
    expect(suggestTransforms("UPPER", "en").some((t) => t.id === "uppercase")).toBe(false);
    expect(suggestTransforms("hello world", "en").some((t) => t.id === "alphanumeric")).toBe(false);
    expect(suggestTransforms("AB-123/45", "en").map((t) => t.id)).toContain("alphanumeric");
  });
});
