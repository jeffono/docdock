import { describe, expect, it } from "vitest";
import { exportDiff } from "./tokens";

const source = ":root {\n  --color-bg: light-dark(white, black);\n  --space-1: 4px;\n}\n";

describe("exportDiff", () => {
  it("replaces only the edited declaration", () => {
    const diff = exportDiff(
      { "--color-bg": "light-dark(gray, silver)", "--space-1": "4px" },
      source,
    );
    expect(diff.split("\n").filter((line) => /^[-+] /.test(line))).toEqual([
      "-  --color-bg: light-dark(white, black);",
      "+  --color-bg: light-dark(gray, silver);",
    ]);
  });

  it("refuses to drop an edit whose source spelling is unexpected", () => {
    expect(() => exportDiff({ "--space-1": "8px" }, ":root {\n  --space-1:  4px ;\n}\n")).toThrow();
  });
});
