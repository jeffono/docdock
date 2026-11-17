import { describe, expect, it } from "vitest";
import type { Listing, Note, PinnedItem } from "../../ipc";
import {
  listingState,
  renamedKnown,
  settledRead,
  shownPinned,
  unreadKnown,
  withoutRemoved,
} from "./listing";

const note = (path: string, modified = 1): Note => ({ path, name: path, modified, created: 1 });
const paths = (notes: Note[]) => notes.map((n) => n.path).sort();
/** The notes shown after a listing, as the list composes them. */
const withUnread = (known: Note[], listing: Listing) => [
  ...listing.notes,
  ...unreadKnown(known, listing),
];
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

// Catches a renamed note lost from a skipped folder while its old name stayed retained.
it("a note renamed before its folder becomes unreadable is kept under its new name", () => {
  const known = [note("a.md"), note("Work/a.md")];
  const renames: [string, string][] = [["Work/a.md", "Work/b.md"]];
  const renamed = renamedKnown(known, renames);
  // The same event twice, or the app's own notice and the watcher's, changes nothing more.
  expect(renamedKnown(renamed, renames)).toEqual(renamed);
  const shown = withUnread(renamed, { notes: [note("a.md")], skipped: ["Work"] });
  expect(paths(shown)).toEqual(["Work/b.md", "a.md"]);
  expect(shown.find((n) => n.path === "Work/b.md")?.name).toBe("b");
});

// Catches a note the app deleted staying listed and searchable because its folder was skipped.
it("a note or folder the app deleted is not kept by a skipped folder", () => {
  const known = [note("a.md"), note("Work/a.md"), note("Work/Old/b.md"), note("Work/c.md")];
  const locked: Listing = { notes: [note("a.md")], skipped: ["Work"] };
  expect(paths(withUnread(withoutRemoved(known, "Work/a.md"), locked))).toEqual([
    "Work/Old/b.md",
    "Work/c.md",
    "a.md",
  ]);
  expect(paths(withUnread(withoutRemoved(known, "Work/Old"), locked))).toEqual([
    "Work/a.md",
    "Work/c.md",
    "a.md",
  ]);
});

// Catches a pinned note in an unreadable folder offered for removal as "not found", losing the pin.
it("a pinned note in a skipped folder is not missing: shown if known, otherwise held back", () => {
  const items = [pin("Work/b.md", true), pin("Work/new.md", true), pin("gone.md", true)];
  expect(shownPinned(items, ["Work"], [note("Work/b.md")])).toEqual([
    pin("Work/b.md", false),
    pin("gone.md", true),
  ]);
});

// Catches a listing taken before a rename or delete putting the old path back, where a skipped
// folder would then keep it.
describe("a listing read while a known note is renamed or deleted", () => {
  /** Lists `first` once, held until released, then `later`; counts the changes as the list does. */
  function lister(first: Listing, later: Listing) {
    let release = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    let reads = 0;
    let changes = 0;
    const read = settledRead(
      async () => {
        reads++;
        if (reads > 1) return later;
        await held;
        return first;
      },
      () => changes,
    );
    return { read, release, change: () => changes++, reads: () => reads };
  }
  const readable: Listing = { notes: [note("Work/a.md")], skipped: [] };
  const locked: Listing = { notes: [], skipped: ["Work"] };

  it("is read again after a rename, so the new path stays", async () => {
    const listing = lister(readable, locked);
    let known = [note("Work/a.md")];
    known = renamedKnown(known, [["Work/a.md", "Work/b.md"]]);
    listing.change();
    listing.release();
    const found = await listing.read;
    expect(listing.reads()).toBe(2);
    expect(paths(withUnread(known, found))).toEqual(["Work/b.md"]);
  });

  it("is read again after a delete, so the note stays gone", async () => {
    const listing = lister(readable, locked);
    let known = [note("Work/a.md")];
    known = withoutRemoved(known, "Work/a.md");
    listing.change();
    listing.release();
    const found = await listing.read;
    expect(listing.reads()).toBe(2);
    expect(withUnread(known, found)).toEqual([]);
  });
});
