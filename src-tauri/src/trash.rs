//! The notes folder's own trash, for deletes the Recycle Bin cannot take, such as on a mounted
//! Cryptomator vault or a network share. It lives in `.docdock/trash`, inside the notes folder and on
//! its volume, so a deleted item never leaves the folder (or the vault) and syncs with it. Items stay
//! `KEEP_MS`, then are purged; Preferences lists them, restores them and empties the trash.
//!
//! Each deleted item is one entry: a folder named `<deleted>-<n>-<random>-<depth>`, each field in
//! lowercase base36 (the deletion time in ms since the Unix epoch, a counter, a random value against
//! collisions between computers syncing one folder, and how many folders lie above the item). The
//! entry holds the item at its original path below the notes folder, so the path and the date need no
//! index file: `a/b/note.md` deleted becomes `<entry>/a/b/note.md`, with depth 2.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::platform;

/// The trash folder's name inside the notes folder's derived-data folder.
pub const DIR: &str = "trash";
/// How long a deleted item stays.
pub const KEEP_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// Deeper paths are not DocDock's: Windows paths cannot nest this far anyway.
const MAX_DEPTH: usize = 128;

/// A random 64-bit value: `RandomState` is seeded by the system's random source.
pub(crate) fn random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(now_ms().into());
    hasher.write_u32(std::process::id());
    hasher.finish()
}
fn base36(mut value: u64) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(value % 36) as usize]);
        value /= 36;
        if value == 0 {
            break;
        }
    }
    digits.reverse();
    String::from_utf8(digits).unwrap_or_default()
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The parts of an entry name that matter.
#[derive(Debug, PartialEq)]
struct Entry {
    deleted: u64,
    depth: usize,
}
fn entry_name(deleted: u64, n: u64, random: u64, depth: usize) -> String {
    format!(
        "{}-{}-{}-{}",
        base36(deleted),
        base36(n),
        base36(random),
        base36(depth as u64)
    )
}
/// None for a name DocDock did not make.
fn parse(name: &str) -> Option<Entry> {
    let number = |text: &str| {
        let digits = (1..=13).contains(&text.len())
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase());
        digits.then(|| u64::from_str_radix(text, 36).ok()).flatten()
    };
    let parts: Vec<&str> = name.split('-').collect();
    let [deleted, n, random, depth] = parts[..] else {
        return None;
    };
    number(n)?;
    number(random)?;
    let depth = usize::try_from(number(depth)?).ok()?;
    (depth <= MAX_DEPTH).then_some(Entry {
        deleted: number(deleted)?,
        depth,
    })
}
/// Whether an entry goes at `now`, from its deletion time (the deleting computer's clock) and its
/// folder's last change (this computer's clock, from when it was made or synced here): both must be
/// over `KEEP_MS` old, so one wrong clock alone cannot purge early. Exactly `KEEP_MS` still stays, and a
/// time ahead of this clock is not old.
fn expired(deleted: u64, modified: u64, now: u64) -> bool {
    now.saturating_sub(deleted.max(modified)) > KEEP_MS
}
fn modified_ms(metadata: &fs::Metadata) -> Option<u64> {
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(modified.as_millis() as u64)
}

/// A deleted item, as Preferences lists it.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Item {
    /// The entry name, which restoring names.
    pub id: String,
    /// Where it was, relative to the notes folder, with `/`. None for an entry whose item cannot be
    /// told, such as one a sync conflict left two items in: it is listed so it can be seen and
    /// emptied, but not restored from the app.
    pub path: Option<String>,
    pub folder: bool,
    /// ms since the Unix epoch.
    pub deleted: u64,
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Moves `target`, a note or a folder at `relative` below the notes folder, into `trash` as a new
/// entry, and answers its id. A rename on the same volume: it either moves whole or stays. `trash` must
/// exist; it is held, with every folder above it, so a link planted under its name is refused rather
/// than followed out of the notes folder. Each folder of the entry is created while the one above it
/// is held, then held itself until the move is done, so none can be swapped for a link meanwhile. The
/// caller holds `target`'s ancestry. A path nested deeper than an entry can name is refused, and the
/// item stays where it is.
pub fn put(trash: &Path, relative: &Path, target: &Path) -> io::Result<String> {
    let names: Vec<_> = relative.iter().collect();
    let (name, folders) = names.split_last().ok_or(io::ErrorKind::InvalidInput)?;
    if folders.len() > MAX_DEPTH {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let _held_trash = platform::hold_ancestry(trash)?;
    let (id, entry) = loop {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let id = entry_name(now_ms(), n, random() >> 24, folders.len());
        let entry = trash.join(&id);
        match fs::create_dir(&entry) {
            Ok(()) => break (id, entry),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    };
    let mut held = Vec::new();
    let moved = fill(&entry, folders, name, target, &mut held);
    // Released first: the entry's folders cannot be removed while held.
    drop(held);
    if let Err(error) = moved {
        // Only the folders made above, still empty: the item stayed where it was.
        discard(&entry);
        return Err(error);
    }
    Ok(id)
}
/// Creates `folders` in `entry`, each held, and moves `target` into the last one as `name`.
fn fill(
    entry: &Path,
    folders: &[&std::ffi::OsStr],
    name: &std::ffi::OsStr,
    target: &Path,
    held: &mut Vec<fs::File>,
) -> io::Result<()> {
    let mut dir = entry.to_owned();
    held.push(platform::hold_directory(&dir)?);
    for folder in folders {
        dir.push(folder);
        fs::create_dir(&dir)?;
        held.push(platform::hold_directory(&dir)?);
    }
    platform::move_new(target, &dir.join(name))
}

/// Files the shell or a sync client may drop into any folder, which never make an item.
fn noise(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy().to_lowercase();
    ["desktop.ini", "thumbs.db", ".ds_store"].contains(&name.as_str())
}
/// The one child of `dir` that `fits` accepts, if exactly one does. Links are never accepted: their
/// file type is neither a folder nor a file.
fn one_child(
    dir: &Path,
    fits: impl Fn(&fs::DirEntry, fs::FileType) -> bool,
) -> Option<fs::DirEntry> {
    let mut found = None;
    for child in fs::read_dir(dir).ok()? {
        let child = child.ok()?;
        let kind = child.file_type().ok()?;
        if fits(&child, kind) && found.replace(child).is_some() {
            return None;
        }
    }
    found
}
/// The item of `entry`, whose name parsed as `parsed`: one chain of `depth` real folders, then one
/// file or folder. Files beside the chain and shell or sync noise are ignored; anything else, such as
/// two candidates, a link or a name that is not Unicode, is no item. Each folder is held, without
/// following links, before it is read, and the chain stays held in the handles answered: none can be
/// swapped for a link while the caller uses the item.
fn item_in(entry: &Path, parsed: &Entry) -> Option<(PathBuf, String, bool, Vec<fs::File>)> {
    let mut dir = entry.to_owned();
    let mut held = vec![platform::hold_directory(&dir).ok()?];
    let mut names = Vec::new();
    for _ in 0..parsed.depth {
        let child = one_child(&dir, |_, kind| kind.is_dir())?;
        names.push(child.file_name().into_string().ok()?);
        dir = child.path();
        held.push(platform::hold_directory(&dir).ok()?);
    }
    let child = one_child(&dir, |child, kind| {
        (kind.is_dir() || kind.is_file()) && !noise(&child.file_name())
    })?;
    let kind = child.file_type().ok()?;
    names.push(child.file_name().into_string().ok()?);
    Some((child.path(), names.join("/"), kind.is_dir(), held))
}

/// Every entry in `trash`, newest first. No trash lists nothing; a trash that is a link fails.
pub fn list(trash: &Path) -> io::Result<Vec<Item>> {
    let _held = match platform::hold_ancestry(trash) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        held => held?,
    };
    let mut items = Vec::new();
    for entry in fs::read_dir(trash)? {
        let entry = entry?;
        let Ok(id) = entry.file_name().into_string() else {
            continue;
        };
        let Some(parsed) = parse(&id) else { continue };
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let item = item_in(&entry.path(), &parsed);
        items.push(Item {
            id,
            folder: item.as_ref().is_some_and(|(_, _, folder, _)| *folder),
            path: item.map(|(_, path, _, _)| path),
            deleted: parsed.deleted,
        });
    }
    items.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| b.id.cmp(&a.id)));
    Ok(items)
}

/// The item of entry `id` in `trash`: where it is now, its original path, whether it is a folder,
/// and the folders above it in the entry, held until the handles drop. `NotFound` when there is no
/// such entry, `InvalidData` when its item cannot be told. The caller holds `trash`'s ancestry.
pub fn find(trash: &Path, id: &str) -> io::Result<(PathBuf, String, bool, Vec<fs::File>)> {
    let parsed = parse(id).ok_or(io::ErrorKind::NotFound)?;
    let entry = trash.join(id);
    if !fs::symlink_metadata(&entry)?.is_dir() {
        return Err(io::ErrorKind::NotFound.into());
    }
    item_in(&entry, &parsed).ok_or_else(|| io::ErrorKind::InvalidData.into())
}

/// Removes entry `id` once its item has moved out: only the folders it held, from the deepest up, and
/// the shell or sync noise in them, so anything else that arrived meanwhile stays. The caller holds
/// `trash`'s ancestry.
pub fn discard_entry(trash: &Path, id: &str) {
    discard(&trash.join(id));
}
/// Each folder is held before it is read or emptied of noise, so a link swapped in never leads
/// outside; it is released only to be removed, which removes a link swapped in then, not its target.
fn discard(entry: &Path) {
    let mut chain = Vec::new();
    let mut dir = entry.to_owned();
    while let Ok(handle) = platform::hold_directory(&dir) {
        let next = one_child(&dir, |_, kind| kind.is_dir());
        chain.push((dir, handle));
        match next {
            Some(child) => dir = child.path(),
            None => break,
        }
    }
    while let Some((dir, handle)) = chain.pop() {
        for child in fs::read_dir(&dir).into_iter().flatten().flatten() {
            if noise(&child.file_name()) && child.file_type().is_ok_and(|kind| kind.is_file()) {
                let _ = fs::remove_file(child.path());
            }
        }
        drop(handle);
        if fs::remove_dir(&dir).is_err() {
            break;
        }
    }
}

/// Deletes for good the entries of `trash` that `expired` lets go at `now`, then the trash itself if
/// nothing is left. Names DocDock did not make stay. Best effort: what cannot be removed now, for
/// example while the vault is locked, goes next time.
pub fn purge(trash: &Path, now: u64) {
    sweep(trash, |entry| {
        let name = entry.file_name();
        let Some(parsed) = name.to_str().and_then(parse) else {
            return false;
        };
        entry
            .metadata()
            .ok()
            .and_then(|metadata| modified_ms(&metadata))
            .is_some_and(|modified| expired(parsed.deleted, modified, now))
    });
}

/// Deletes for good the entries `ids` of `trash`, the ones the user saw listed, then the trash
/// itself if nothing is left. Anything else, such as an entry synced in since, stays.
pub fn empty(trash: &Path, ids: &[String]) -> io::Result<()> {
    sweep(trash, |entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| ids.iter().any(|id| id == name))
    })
    .map_or(Ok(()), |kind| Err(kind.into()))
}

/// Removes the entries of `trash` that `goes` accepts, links without following them. A trash that is
/// a link is left alone. Answers the kind of the first failure, if any.
fn sweep(trash: &Path, goes: impl Fn(&fs::DirEntry) -> bool) -> Option<io::ErrorKind> {
    let mut failed = None;
    {
        let held = platform::hold_ancestry(trash);
        let entries = held.and_then(|held| fs::read_dir(trash).map(|entries| (held, entries)));
        let (_held, entries) = match entries {
            Ok(found) => found,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => return Some(error.kind()),
        };
        for entry in entries.flatten() {
            if !goes(&entry) {
                continue;
            }
            if let Err(error) = remove(&entry.path()) {
                eprintln!("[trash] removal failed: {:?}", error.kind());
                failed.get_or_insert(error.kind());
            }
        }
    }
    // Not recursive: an entry that stays keeps it.
    let _ = fs::remove_dir(trash);
    failed
}

/// Deletes `path` and everything below it, removing links without following them. Each folder is held
/// before it is read and released only to be removed, so a link swapped in meanwhile is removed as a
/// link, never followed. Walks the tree itself: `fs::remove_dir_all` asks for POSIX delete
/// semantics, which a vault mounted by WinFsp refuses with ERROR_INVALID_PARAMETER after deleting the
/// first file.
fn remove(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let unlink = || fs::remove_file(path).or_else(|_| fs::remove_dir(path));
    if metadata.is_symlink() || !metadata.is_dir() {
        // A link to a folder is itself a directory entry on Windows.
        return unlink();
    }
    {
        let Ok(_held) = platform::hold_directory(path) else {
            // No longer a real folder, such as a link swapped in since.
            return unlink();
        };
        for entry in fs::read_dir(path)? {
            remove(&entry?.path())?;
        }
    }
    fs::remove_dir(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000;

    #[test]
    fn entry_names_carry_the_deletion_time_and_depth() {
        let name = entry_name(NOW, 36, 7, 2);
        assert_eq!(name, format!("{}-10-7-2", base36(NOW)));
        assert_eq!(
            parse(&name),
            Some(Entry {
                deleted: NOW,
                depth: 2
            })
        );
    }
    #[test]
    fn names_docdock_did_not_make_are_not_entries() {
        for name in [
            "dropped here by hand.md",
            "mz2xp0w0-0-0",
            "mz2xp0w0-0-0-0-0",
            "MZ2XP0W0-0-0-0",
            "mz2xp0w0-0--0",
            "mz2xp0w0-0-0-zz",
            "mz2xp0w0 a-0-0-0",
        ] {
            assert_eq!(parse(name), None, "{name}");
        }
    }
    #[test]
    fn items_stay_exactly_thirty_days() {
        let old = NOW - KEEP_MS - 1;
        assert!(!expired(NOW - KEEP_MS, old, NOW));
        assert!(expired(old, old, NOW));
        // A clock ahead of this one.
        assert!(!expired(NOW + 1000, old, NOW));
    }
    #[test]
    fn a_found_items_folders_stay_held_until_released() {
        let trash = std::env::temp_dir().join(format!("docdock-trash-held-{}", random()));
        fs::create_dir_all(trash.join("1-0-0-1").join("sub")).unwrap();
        fs::write(trash.join("1-0-0-1").join("sub").join("a.md"), "a").unwrap();
        let (_, path, _, held) = find(&trash, "1-0-0-1").unwrap();
        assert_eq!(path, "sub/a.md");
        // Held folders cannot be swapped: neither renamed away nor replaced.
        let sub = trash.join("1-0-0-1").join("sub");
        assert!(fs::rename(&sub, trash.join("moved")).is_err());
        assert!(fs::rename(trash.join("1-0-0-1"), trash.join("moved")).is_err());
        drop(held);
        assert!(fs::rename(&sub, trash.join("moved")).is_ok());
        let _ = remove(&trash);
    }
    #[test]
    fn an_old_deletion_time_alone_does_not_purge() {
        // Deleted by a computer whose clock ran late, but made or synced here recently.
        assert!(!expired(0, NOW - 1000, NOW));
    }
}
