import { expect, it } from "vitest";
import { parseAddress } from "./address";

it("recognizes web addresses without turning ordinary queries into navigation", () => {
  expect(parseAddress(" example.com/path?q=1 ")).toBe("https://example.com/path?q=1");
  expect(parseAddress("http://localhost:8080")).toBe("http://localhost:8080/");
  for (const query of [
    "my note",
    "notes",
    "javascript:alert(1)",
    "file:///c:/a",
    "mailto:a@b.com",
    "user@example.com",
    "https://user:password@example.com",
  ])
    expect(parseAddress(query)).toBeNull();
});
