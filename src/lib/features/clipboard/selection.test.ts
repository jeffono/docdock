import { describe, expect, it } from "vitest";
import { newestNewId } from "./selection";

const entries = (...ids: string[]) => ids.map((id) => ({ id }));

describe("newest clipboard item", () => {
  it("selects the top item on the first snapshot", () => {
    expect(newestNewId([], entries("newest", "older"))).toBe("newest");
  });

  it("selects a new top item", () => {
    expect(newestNewId(entries("older"), entries("newest", "older"))).toBe("newest");
  });

  it("keeps an existing top item when a new item appears lower in the list", () => {
    expect(newestNewId(entries("newest"), entries("newest", "older"))).toBeNull();
  });

  it("keeps a known item that moved to the top", () => {
    expect(newestNewId(entries("first", "second"), entries("second", "first"))).toBeNull();
  });

  it("does not select an item from an empty snapshot", () => {
    expect(newestNewId(entries("newest"), [])).toBeNull();
  });
});
