# 0013 — Trash entries that hold a note with its images

**Context:** deleting a note now also takes the images in `attachments/` folders that only it
references. Where the Recycle Bin cannot take them (a vault, a network share), they go to the notes
folder's own trash (`.docdock/trash`, [0009](0009-note-privacy.md)), which syncs with the folder and
may be read by older DocDock versions on other computers. Until now an entry held exactly one item:
a folder named `<deleted>-<n>-<random>-<depth>` with the item at its original path, the depth naming
the chain of folders down to it, and no index file. A note and its images restored separately would
leave the note's links broken until the user found each image in Preferences.

**Decision:**

- **One entry:** the note and the images that went with it are one entry. The note stays at its
  original path in the entry, as before; the images go in the entry's `.images` folder, each at its
  own original path below the notes folder: `a/b/note.md` with `a/attachments/map.png` becomes
  `<entry>/a/b/note.md` and `<entry>/.images/a/attachments/map.png`. The entry name keeps its four
  fields and the depth still names the note. No item path starts with a dot, so `.images` is never
  part of the note's chain; an entry without it is an old entry and reads as before.
- **Restore:** the note returns as any item does, under a free name such as `note (2).md` when its
  own is taken, in the same folder, where its relative links still hold. Each image returns to its
  own original path. Every image's destination is checked before anything moves: if any image's
  name is taken, by a file or by a Recycle Bin Undo's reservation, or a file took one of its
  folders' names, nothing moves and the restore answers `restoreTaken` naming it: under a free name
  the note would silently show another image or none. The user renames that file and restores again.
  An image only ever returns under its own name: one whose name is taken in the instant between that
  check and its move, or that cannot move, such as one another app holds open, stays in the entry,
  and the restore answers `imagesLeftInTrash` naming it while the note is back. The entry then holds
  only its `.images` folder: it lists under the first image's path, restores every image to its own
  path under the same check, and counts its 30 days from that restore, the last change to its
  folder. None is ever overwritten nor deleted.
- **Recycle Bin Undo** follows the same rule: a taken image or folder name answers `restoreTaken`,
  nothing changes and Undo stays possible until the panel hides. An image that still cannot be
  written under its own name keeps its bytes in Undo's memory and its name reserved, since the bin
  may have been emptied, and Undo answers `imagesLeftInBin`; Undo again retries it.
- **Emptying and purging** the trash take the save lock, as a restore does, so neither deletes an
  entry a restore is moving.

**Consequences:**

- An older DocDock version sees a new entry as two candidates (the note's chain and `.images`) and
  lists it as an unrecognized item: it cannot restore it, but empties and purges it as any other.
  Nothing it does splits the note from its images.
- The unused-images scan and the delete's own reference scan read notes in the trash at their
  original path by dropping the entry folder; images in `.images` are never notes, so neither scan
  changes for them, and they are never candidates for the unused-images cleanup.
- An older DocDock version lists an entry left with only images as the folder `.images` or one below
  it, and refuses to restore it, since it restores no folder whose name starts with a dot; it
  empties and purges it as any other.
- Emptying or purging an entry deletes the note and its images together, as one item.
