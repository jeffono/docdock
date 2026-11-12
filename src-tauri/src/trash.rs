//! The notes folder's own trash, for deletes the Recycle Bin cannot take, such as on a mounted
//! Cryptomator vault or a network share. A deleted item moves to `.docdock-trash` at the root of the
//! notes folder, on the same volume, so it never leaves the folder (or the vault). Hidden folders are
//! skipped by listing, search, tags and the watcher. Entries are named after the panel session that
//! made them, and a purge removes every entry from another session or process: what remains of a
//! session that ended, or of a run that did not end cleanly.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::platform;

pub const DIR: &str = ".docdock-trash";

/// The prefix every entry of this process's `session` starts with.
fn prefix(pid: u32, session: u64) -> String {
    format!("{pid}-{session}-")
}
/// `<pid>-<session>-<n> <name>`: unique within the session, and the original name stays readable.
fn entry_name(pid: u32, session: u64, n: u64, name: &OsStr) -> OsString {
    let mut entry = OsString::from(format!("{}{n} ", prefix(pid, session)));
    entry.push(name);
    entry
}
/// Whether a purge in this process's `session` removes `entry`: anything another session made.
fn purgeable(entry: &OsStr, pid: u32, session: u64) -> bool {
    !entry.to_string_lossy().starts_with(&prefix(pid, session))
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Moves `target`, a note or a folder below `root`, into the trash, and answers where it landed. A
/// rename on the same volume: it either moves whole or stays. The trash folder is created if needed
/// and held, so a link planted under its name is refused rather than followed out of the folder.
/// The caller holds `target`'s ancestry.
pub fn put(root: &Path, target: &Path, session: u64) -> io::Result<PathBuf> {
    let name = target.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let trash = root.canonicalize()?.join(DIR);
    match fs::create_dir(&trash) {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => return Err(error),
        _ => {}
    }
    let _held = platform::hold_ancestry(&trash)?;
    let pid = std::process::id();
    loop {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let landed = trash.join(entry_name(pid, session, n, name));
        match platform::move_new(target, &landed) {
            Ok(()) => return Ok(landed),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

/// Whether `path` is an entry of `root`'s trash, the only place Undo moves a folder back from.
pub fn holds(root: &Path, path: &Path) -> bool {
    root.canonicalize()
        .is_ok_and(|root| path.parent() == Some(root.join(DIR).as_path()))
}

/// Deletes for good the entries of `root`'s trash that `session` did not make, then the trash itself
/// if nothing is left. Links inside are removed, never followed; a trash that is a link is left alone.
/// Best effort: what cannot be removed now, for example while the vault is locked, goes next time.
pub fn purge(root: &Path, session: u64) {
    let trash = root.join(DIR);
    let pid = std::process::id();
    {
        let Ok(_held) = platform::hold_ancestry(&trash) else {
            return;
        };
        let Ok(entries) = fs::read_dir(&trash) else {
            return;
        };
        for entry in entries.flatten() {
            if !purgeable(&entry.file_name(), pid, session) {
                continue;
            }
            let path = entry.path();
            let removed = match fs::symlink_metadata(&path) {
                Ok(m) if m.is_dir() => fs::remove_dir_all(&path),
                // A link to a folder is itself a directory entry on Windows.
                Ok(m) if m.is_symlink() => {
                    fs::remove_file(&path).or_else(|_| fs::remove_dir(&path))
                }
                Ok(_) => fs::remove_file(&path),
                Err(error) => Err(error),
            };
            if let Err(error) = removed {
                eprintln!("[trash] {} not purged: {error}", path.display());
            }
        }
    }
    // Not recursive: an entry of the current session keeps it.
    let _ = fs::remove_dir(&trash);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_purge_keeps_only_the_current_sessions_entries() {
        let name = entry_name(42, 7, 3, OsStr::new("note.md"));
        assert_eq!(name, "42-7-3 note.md");
        assert!(!purgeable(&name, 42, 7));
        // An ended session, another run and a session whose number only starts the same.
        assert!(purgeable(&name, 42, 8));
        assert!(purgeable(&name, 43, 7));
        assert!(purgeable(&entry_name(42, 70, 0, OsStr::new("a.md")), 42, 7));
        assert!(purgeable(OsStr::new("dropped here by hand.md"), 42, 7));
    }
}
