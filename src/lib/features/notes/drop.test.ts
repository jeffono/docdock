import { describe, expect, it } from "vitest";
import { appended, droppedContent } from "./drop";

const transfer = (files: File[], text = "") =>
  ({ files, getData: () => text }) as unknown as DataTransfer;

describe("appended", () => {
  // Catches a drop that eats a note's last line or glues the new text to it.
  it("adds the block as its own paragraph after the note, untouched", () => {
    expect(appended("Line", "New")).toBe("Line\n\nNew\n");
    expect(appended("Line\n", "New")).toBe("Line\n\nNew\n");
    expect(appended("Line\n\n", "New")).toBe("Line\n\nNew\n");
    expect(appended("", "New\n")).toBe("New\n");
  });

  it("keeps the note's line endings", () => {
    expect(appended("a\r\nb\r\n", "x\ny")).toBe("a\r\nb\r\n\r\nx\r\ny\r\n");
  });
});

describe("droppedContent", () => {
  const png = new File(["x"], "a.png", { type: "image/png" });

  // Catches a browser image saved as its address, which browsers add as text.
  it("takes images over the text that comes with them", () => {
    expect(droppedContent(transfer([png], "https://e.com/a.png"))).toEqual({ images: [png] });
  });

  it("refuses files that are not images instead of attaching them", () => {
    const pdf = new File(["x"], "a.pdf", { type: "application/pdf" });
    expect(droppedContent(transfer([png, pdf]))).toBe("unsupported");
  });

  it("takes plain text with its line breaks normalized, and nothing for blank text", () => {
    expect(droppedContent(transfer([], "a\r\nb"))).toEqual({ text: "a\nb" });
    expect(droppedContent(transfer([], " \n"))).toBeNull();
  });
});
