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
  own original path. If any image's name is taken, by a file or by a Recycle Bin Undo's reservation,
  nothing moves and the restore answers `restoreTaken` naming that image: under a free name the note
  would silently show another image or none. The user renames that file and restores again. An
  image whose name is taken in the instant between that check and its move takes a free name, and
  one that cannot move stays in the entry; neither is ever overwritten nor deleted.
- **Recycle Bin Undo** follows the same rule: a taken image name answers `restoreTaken`, nothing
  changes and Undo stays possible until the panel hides.

**Consequences:**

- An older DocDock version sees a new entry as two candidates (the note's chain and `.images`) and
  lists it as an unrecognized item: it cannot restore it, but empties and purges it as any other.
  Nothing it does splits the note from its images.
- The unused-images scan and the delete's own reference scan read notes in the trash at their
  original path by dropping the entry folder; images in `.images` are never notes, so neither scan
  changes for them, and they are never candidates for the unused-images cleanup.
- An entry left with only images, which only a failed move can cause, lists as an unrecognized item.
- Emptying or purging an entry deletes the note and its images together, as one item.
