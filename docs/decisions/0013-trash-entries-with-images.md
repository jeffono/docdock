# 0013 — Trash entries that hold a note with its images

**Context:** deleting a note now also takes the images in `attachments/` folders that only it
references. Moving them at once and keeping their bytes in memory for Undo needed reserved names,
retries and partial restores, and every review of that machinery found rare cases that could lose an
image. Where the Recycle Bin cannot take them (a vault, a network share), they go to the notes
folder's own trash (`.docdock/trash`, [0009](0009-note-privacy.md)), which syncs with the folder and
may be read by older DocDock versions on other computers. Until now an entry held exactly one item: a
folder named `<deleted>-<n>-<random>-<depth>` with the item at its original path, the depth naming
the chain of folders down to it, and no index file. A note and its images restored separately would
leave the note's links broken until the user found each image in Preferences.

**Decision:**

- **Deferred:** the delete moves only the note, as before, and records the paths of its images as
  pending for the panel session. Undo needs nothing for them: they never left. When the panel hides,
  if the folder is still the one chosen and available, the images that no note references by then,
  live or in the trash, except the deleted note in its own entry, follow it. Any doubt or error keeps
  an image in place, where "Unused images" lists it; a folder going away or the app closing first
  keeps them all.
- **One entry:** where the note went to the trash, its images join its entry, in the entry's
  `.images` folder, each at its own original path below the notes folder: `a/b/note.md` with
  `a/attachments/map.png` becomes `<entry>/a/b/note.md` and `<entry>/.images/a/attachments/map.png`.
  The entry name keeps its four fields and the depth still names the note. No item path starts with
  a dot, so `.images` is never part of the note's chain; an entry without it is an old entry and
  reads as before. If the entry is gone by then (restored or emptied), or the note went to the
  Recycle Bin, the images go as "Send to bin" sends them: the bin, else entries of their own.
- **Restore:** the note returns as any item does, under a free name such as `note (2).md` when its
  own is taken, in the same folder, where its relative links still hold. Each image returns only
  under its own original path, never a free name the note's link would miss, never over a file and
  never through a link. One whose name is taken, or that cannot move, stays in the entry, and the
  restore answers `imagesLeftInTrash` naming it while the note is back. The entry then holds only its
  `.images` folder, beside any folders the restore emptied but could not remove: it lists under the
  first image's path, restores every image to its own path the same way, and counts its 30 days from
  that restore, the last change to its folder. None is ever overwritten nor deleted.
- **Emptying and purging** the trash take the save lock, as a restore and the session's end do, so
  neither deletes an entry while something moves in or out of it.

**Consequences:**

- The notice counts the images at once, but they move only when the panel hides; an image a note
  starts using before then stays.
- An older DocDock version sees an entry with images as two candidates (the note's chain and
  `.images`) and lists it as an unrecognized item: it cannot restore it, but empties and purges it as
  any other. Nothing it does splits the note from its images.
- The unused-images scan and the delete's own reference scan read notes in the trash at their
  original path by dropping the entry folder; images in `.images` are never notes, so neither scan
  changes for them, and they are never candidates for the unused-images cleanup.
- An older DocDock version lists an entry left with only images as the folder `.images` or one below
  it, and refuses to restore it, since it restores no folder whose name starts with a dot; it
  empties and purges it as any other.
- Emptying or purging an entry deletes the note and its images together, as one item.
