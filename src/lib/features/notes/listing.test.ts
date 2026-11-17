import { describe, expect, it } from "vitest";
import type { Listing, Note, PinnedItem } from "../../ipc";
import { listingState, shownPinned, withUnread } from "./listing";

const note = (path: string, modified = 1): Note => ({ path, name: path, modified, created: 1 });
const paths = (notes: Note[]) => notes.map((n) => n.path).sort();
const pin = (target: string, missing: boolean): PinnedItem => ({
  target,
  title: target,
  kind: "note",
  missing,
  collapsed: false,
});

describe("a subfolder that cannot be read for a while", () => {
  // Catches a partial listing taken for deletion: the edited note turned `deleted` and its next
  // save conflicted with the file still on disk.
  it("keeps its notes as they were, and the next readable listing decides again", () => {
    const full: Listing = {
      notes: [note("a.md"), note("Work/b.md"), note("Work/c.md")],
      skipped: [],
    };
    let shown = withUnread([], full);
    expect(paths(shown)).toEqual(["Work/b.md", "Work/c.md", "a.md"]);

    const locked: Listing = { notes: [note("a.md", 2)], skipped: ["Work"] };
    shown = withUnread(shown, locked);
    expect(shown).toEqual([note("a.md", 2), note("Work/b.md"), note("Work/c.md")]);
    expect(listingState("Work/b.md", locked)).toBe("unread");

    // c.md was deleted while its folder could not be read.
    const readable: Listing = { notes: [note("a.md", 2), note("Work/b.md", 3)], skipped: [] };
    shown = withUnread(shown, readable);
    expect(shown).toEqual(readable.notes);
    expect(listingState("Work/b.md", readable)).toBe("listed");
    expect(listingState("Work/c.md", readable)).toBe("gone");
  });

  it("keeps the unlisted rest of a folder that failed partway", () => {
    const known = [note("Work/b.md"), note("Work/c.md")];
    const partial: Listing = { notes: [note("Work/b.md", 2)], skipped: ["Work"] };
    expect(withUnread(known, partial)).toEqual([note("Work/b.md", 2), note("Work/c.md")]);
  });
});

it("a note deleted from a folder that was read is gone, even while another folder is skipped", () => {
  const listing: Listing = { notes: [note("a.md")], skipped: ["Work"] };
  expect(
    paths(withUnread([note("a.md"), note("Home/d.md"), note("Workshop/e.md")], listing)),
  ).toEqual(["a.md"]);
  expect(listingState("Home/d.md", listing)).toBe("gone");
  // A sibling whose name only starts like the skipped folder was read.
  expect(listingState("Workshop/e.md", listing)).toBe("gone");
});

it("matches a skipped folder ignoring case, and below it at any depth", () => {
  const listing: Listing = { notes: [], skipped: ["Work/Old"] };
  expect(listingState("work/old/deep/x.md", listing)).toBe("unread");
  expect(listingState("Work/x.md", listing)).toBe("gone");
});

// Catches a pinned note in an unreadable folder offered for removal as "not found", losing the pin.
it("a pinned note in a skipped folder is not missing: shown if known, otherwise held back", () => {
  const items = [pin("Work/b.md", true), pin("Work/new.md", true), pin("gone.md", true)];
  expect(shownPinned(items, ["Work"], [note("Work/b.md")])).toEqual([
    pin("Work/b.md", false),
    pin("gone.md", true),
  ]);
});
