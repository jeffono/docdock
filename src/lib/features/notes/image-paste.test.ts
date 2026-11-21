import { expect, test } from "vitest";
import { imageInsertion } from "./image-paste";

const attachment = "attachments/new%20photo.png";
const image = `![](${attachment})`;

test("ordinary text around a selection does not add separators", () => {
  expect(imageInsertion("before ", " after", [attachment])).toEqual({
    insert: image,
    cursorOffset: image.length,
  });
});

test("a drop joins all attachments in order with single newlines", () => {
  const insert = `${image}\n![](second.png)\n![](third.png)`;
  expect(imageInsertion("", "", [attachment, "second.png", "third.png"])).toEqual({
    insert,
    cursorOffset: insert.length,
  });
});

test.each(["![](previous.png)", '![Previous](<a (b).png> "title") \t'])(
  "a paste after %s starts on the next line",
  (before) => {
    expect(imageInsertion(before, "", [attachment])).toEqual({
      insert: `\n${image}`,
      cursorOffset: image.length + 1,
    });
  },
);

test("a paste before an image leaves the cursor before the added newline", () => {
  const after = " \t![Following](a(b).png) and text";
  const pasted = imageInsertion("", after, [attachment]);
  expect(pasted).toEqual({ insert: `${image}\n`, cursorOffset: image.length });
  const line = pasted.insert.slice(0, pasted.cursorOffset);
  const next = imageInsertion(line, "", ["second.png"]);
  expect(line + next.insert + pasted.insert.slice(pasted.cursorOffset) + after).toBe(
    `${image}\n![](second.png)\n${after}`,
  );
});

test("a drop between adjacent images adds one newline on each side", () => {
  const inserted = `\n${image}\n![](second.png)`;
  expect(imageInsertion("![](before.png)", "![](after.png)", [attachment, "second.png"])).toEqual({
    insert: `${inserted}\n`,
    cursorOffset: inserted.length,
  });
});

test.each([
  "[link](a.png)",
  "\\![](a.png)",
  "`![](a.png)`",
  "![incomplete](bad dest)",
  "![](a.png) x",
])("a paste after %s does not see a neighboring image", (before) => {
  expect(imageInsertion(before, "", [attachment]).insert).toBe(image);
});

test("a reference image before parenthesized text does not count as an inline image", () => {
  expect(imageInsertion("", "![reference][label](text)", [attachment]).insert).toBe(image);
});
