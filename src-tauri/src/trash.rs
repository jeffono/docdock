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
/// Whether an entry deleted at `deleted` goes at `now`: after exactly `KEEP_MS` it still stays. A
/// deletion time ahead of this clock is not old.
fn expired(deleted: u64, now: u64) -> bool {
    now.saturating_sub(deleted) > KEEP_MS
}

/// A deleted item, as Preferences lists it.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Item {
    /// The entry name, which restoring names.
    pub id: String,
    /// Where it was, relative to the notes folder, with `/`.
    pub path: String,
    pub folder: bool,
    /// ms since the Unix epoch.
    pub deleted: u64,
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Moves `target`, a note or a folder at `relative` below the notes folder, into `trash` as a new
/// entry, and answers its id. A rename on the same volume: it either moves whole or stays. `trash` must
/// exist; it is held, with every folder above it, so a link planted under its name is refused rather
/// than followed out of the notes folder. The caller holds `target`'s ancestry.
pub fn put(trash: &Path, relative: &Path, target: &Path) -> io::Result<String> {
    let names: Vec<_> = relative.iter().collect();
    let (name, folders) = names.split_last().ok_or(io::ErrorKind::InvalidInput)?;
    let _held = platform::hold_ancestry(trash)?;
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
    let moved = (|| {
        let mut dir = entry.clone();
        for folder in folders {
            dir.push(folder);
            fs::create_dir(&dir)?;
        }
        let _held = platform::hold_ancestry(&dir)?;
        platform::move_new(target, &dir.join(name))
    })();
    if let Err(error) = moved {
        // Only the folders made above, still empty: the item stayed where it was.
        discard(&entry);
        return Err(error);
    }
    Ok(id)
}

/// The item of `entry`, whose name parsed as `parsed`: the entry holds one chain of `depth` real
/// folders, then one file or folder. Links are never followed; anything else is not an item.
fn item_in(entry: &Path, parsed: &Entry) -> Option<(PathBuf, String, bool)> {
    let only = |dir: &Path| -> Option<fs::DirEntry> {
        let mut children = fs::read_dir(dir).ok()?;
        let child = children.next()?.ok()?;
        children.next().is_none().then_some(child)
    };
    let mut dir = entry.to_owned();
    let mut names = Vec::new();
    for _ in 0..parsed.depth {
        let child = only(&dir)?;
        if !child.file_type().ok()?.is_dir() {
            return None;
        }
        names.push(child.file_name().into_string().ok()?);
        dir = child.path();
    }
    let child = only(&dir)?;
    let kind = child.file_type().ok()?;
    if !kind.is_dir() && !kind.is_file() {
        return None;
    }
    names.push(child.file_name().into_string().ok()?);
    Some((child.path(), names.join("/"), kind.is_dir()))
}

/// Every item in `trash`, newest first. No trash lists nothing; a trash that is a link fails.
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
        if let Some((_, path, folder)) = item_in(&entry.path(), &parsed) {
            items.push(Item {
                id,
                path,
                folder,
                deleted: parsed.deleted,
            });
        }
    }
    items.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| b.id.cmp(&a.id)));
    Ok(items)
}

/// The item of entry `id` in `trash`, and where it is now. `NotFound` when there is no such item.
/// The caller holds `trash`'s ancestry.
pub fn find(trash: &Path, id: &str) -> io::Result<(PathBuf, Item)> {
    let parsed = parse(id).ok_or(io::ErrorKind::NotFound)?;
    let entry = trash.join(id);
    if !fs::symlink_metadata(&entry)?.is_dir() {
        return Err(io::ErrorKind::NotFound.into());
    }
    let (source, path, folder) = item_in(&entry, &parsed).ok_or(io::ErrorKind::NotFound)?;
    Ok((
        source,
        Item {
            id: id.to_owned(),
            path,
            folder,
            deleted: parsed.deleted,
        },
    ))
}

/// Removes entry `id` once its item has moved out: only the empty folders it held, from the deepest
/// up, so anything that arrived meanwhile stays.
pub fn discard_entry(trash: &Path, id: &str) {
    discard(&trash.join(id));
}
fn discard(entry: &Path) {
    let mut chain = Vec::new();
    let mut dir = entry.to_owned();
    loop {
        let next = fs::read_dir(&dir)
            .ok()
            .and_then(|mut children| children.next())
            .and_then(Result::ok)
            .filter(|child| child.file_type().is_ok_and(|kind| kind.is_dir()));
        chain.push(dir);
        match next {
            Some(child) => dir = child.path(),
            None => break,
        }
    }
    for dir in chain.iter().rev() {
        if fs::remove_dir(dir).is_err() {
            break;
        }
    }
}

/// Deletes for good the entries of `trash` older than `KEEP_MS` at `now`, then the trash itself if
/// nothing is left. Names DocDock did not make stay. Best effort: what cannot be removed now, for
/// example while the vault is locked, goes next time.
pub fn purge(trash: &Path, now: u64) {
    sweep(trash, |name| {
        parse(name).is_some_and(|entry| expired(entry.deleted, now))
    });
}

/// Deletes for good everything in `trash`, then the trash itself.
pub fn empty(trash: &Path) -> io::Result<()> {
    sweep(trash, |_| true).map_or(Ok(()), |kind| Err(kind.into()))
}

/// Removes the entries of `trash` that `goes` accepts, links without following them. A trash that is
/// a link is left alone. Answers the kind of the first failure, if any.
fn sweep(trash: &Path, goes: impl Fn(&str) -> bool) -> Option<io::ErrorKind> {
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
            if !goes(&entry.file_name().to_string_lossy()) {
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

/// Deletes `path` and everything below it, removing links without following them. Walks the tree
/// itself: `fs::remove_dir_all` asks for POSIX delete semantics, which a vault mounted by WinFsp
/// refuses with ERROR_INVALID_PARAMETER after deleting the first file.
fn remove(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_symlink() {
        // A link to a folder is itself a directory entry on Windows.
        return fs::remove_file(path).or_else(|_| fs::remove_dir(path));
    }
    if !metadata.is_dir() {
        return fs::remove_file(path);
    }
    for entry in fs::read_dir(path)? {
        remove(&entry?.path())?;
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
        assert!(!expired(NOW - KEEP_MS, NOW));
        assert!(expired(NOW - KEEP_MS - 1, NOW));
        // A clock ahead of this one.
        assert!(!expired(NOW + 1000, NOW));
    }
}
