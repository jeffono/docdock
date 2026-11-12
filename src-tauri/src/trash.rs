//! The notes folder's own trash, for deletes the Recycle Bin cannot take, such as on a mounted
//! Cryptomator vault or a network share. A deleted item moves to `.docdock-trash` at the root of the
//! notes folder, on the same volume, so it never leaves the folder (or the vault). Hidden folders are
//! skipped by listing, search, tags and the watcher.
//!
//! Entries are named `<install>-<run>-<session>-<created>-<n> <name>`: the installation that made
//! them (a random id kept in the app settings), the run, the panel session, the creation time in ms
//! since the Unix epoch and a counter. A synced notes folder may hold another computer's entries, and
//! that computer's panel may still offer their Undo, so the name says who may remove what:
//! - this run's entries go once their session has ended, never while it is the live one;
//! - an earlier run of this installation has ended (one instance per computer), so its entries go;
//! - another installation's entries go only after `FOREIGN_AGE`;
//! - a name in no such form was not made by DocDock and stays.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::platform;

pub const DIR: &str = ".docdock-trash";
/// How long another computer's entries stay: far longer than a panel stays open with an Undo.
const FOREIGN_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Who makes entries: this installation and this run, as lowercase hex.
#[derive(Clone, Debug, PartialEq)]
pub struct Owner {
    install: String,
    run: String,
}
static OWNER: OnceLock<Owner> = OnceLock::new();

/// A random 64-bit value: `RandomState` is seeded by the system's random source.
fn random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(now_ms().into());
    hasher.write_u32(std::process::id());
    hasher.finish()
}
/// A new installation id, to keep in the settings.
pub fn new_install_id() -> String {
    format!("{:016x}", random())
}
/// Whether `id` is one `new_install_id` could have made.
pub fn valid_install_id(id: &str) -> bool {
    is_hex(id, 16)
}
/// Sets this installation's id, once, before the first entry; without it, an id for this run only.
pub fn set_install(install: String) {
    let _ = OWNER.set(Owner {
        install,
        run: format!("{:08x}", random() as u32),
    });
}
fn owner() -> &'static Owner {
    OWNER.get_or_init(|| Owner {
        install: new_install_id(),
        run: format!("{:08x}", random() as u32),
    })
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn entry_name(owner: &Owner, session: u64, created: u64, n: u64, name: &OsStr) -> OsString {
    let mut entry = OsString::from(format!(
        "{}-{}-{session}-{created}-{n} ",
        owner.install, owner.run
    ));
    entry.push(name);
    entry
}
/// The parts of an entry name, or None for a name DocDock did not make.
struct Entry<'a> {
    install: &'a str,
    run: &'a str,
    session: u64,
    created: u64,
}
fn parse(name: &str) -> Option<Entry<'_>> {
    let (prefix, _) = name.split_once(' ')?;
    let parts: Vec<&str> = prefix.split('-').collect();
    let [install, run, session, created, n] = parts[..] else {
        return None;
    };
    let number = |text: &str| {
        (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<u64>().ok())
            .flatten()
    };
    number(n)?;
    (is_hex(install, 16) && is_hex(run, 8)).then_some(Entry {
        install,
        run,
        session: number(session)?,
        created: number(created)?,
    })
}
/// Whether a purge by `me`, whose live panel session is `live`, removes the entry `name` at `now`
/// (ms since the Unix epoch). See the module comment.
fn purgeable(name: &OsStr, me: &Owner, live: u64, now: u64) -> bool {
    let name = name.to_string_lossy();
    let Some(entry) = parse(&name) else {
        return false;
    };
    if entry.install != me.install {
        // A creation time ahead of this clock is not old.
        now.saturating_sub(entry.created) > FOREIGN_AGE_MS
    } else {
        entry.run != me.run || entry.session < live
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Moves `target`, a note or a folder below `root`, into the trash as an entry of `session`, and
/// answers where it landed. A rename on the same volume: it either moves whole or stays. The trash
/// folder is created if needed and held, so a link planted under its name is refused rather than
/// followed out of the folder. The caller holds `target`'s ancestry.
pub fn put(root: &Path, target: &Path, session: u64) -> io::Result<PathBuf> {
    let name = target.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let trash = root.canonicalize()?.join(DIR);
    match fs::create_dir(&trash) {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => return Err(error),
        _ => {}
    }
    let _held = platform::hold_ancestry(&trash)?;
    loop {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let landed = trash.join(entry_name(owner(), session, now_ms(), n, name));
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

/// Deletes for good the entries of `root`'s trash that may go, then the trash itself if nothing is
/// left. `live` answers the live panel session, read again for each entry: a session that is live
/// when the entry is judged may still offer its Undo, and one that has ended never will again, so a
/// purge started long ago cannot take a newer session's entry. Links inside are removed, never
/// followed; a trash that is a link is left alone. Best effort: what cannot be removed now, for
/// example while the vault is locked, goes next time.
pub fn purge(root: &Path, live: impl Fn() -> u64) {
    let trash = root.join(DIR);
    {
        let Ok(_held) = platform::hold_ancestry(&trash) else {
            return;
        };
        let Ok(entries) = fs::read_dir(&trash) else {
            return;
        };
        for entry in entries.flatten() {
            if !purgeable(&entry.file_name(), owner(), live(), now_ms()) {
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
    // Not recursive: an entry that stays keeps it.
    let _ = fs::remove_dir(&trash);
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn other_run() -> Owner {
        Owner {
            install: owner().install.clone(),
            run: if owner().run == "00000000" {
                "00000001"
            } else {
                "00000000"
            }
            .into(),
        }
    }
    /// An entry name as another run of this installation would make it.
    pub fn earlier_run_entry(name: &str) -> OsString {
        entry_name(&other_run(), 0, now_ms(), 0, OsStr::new(name))
    }
    fn me() -> Owner {
        Owner {
            install: "0123456789abcdef".into(),
            run: "89abcdef".into(),
        }
    }
    const NOW: u64 = 1_800_000_000_000;

    #[test]
    fn entry_names_carry_their_owner_and_keep_the_original_name() {
        let name = entry_name(&me(), 7, NOW, 3, OsStr::new("a b.md"));
        assert_eq!(name, "0123456789abcdef-89abcdef-7-1800000000000-3 a b.md");
    }
    #[test]
    fn this_runs_entries_go_only_once_their_session_has_ended() {
        let name = entry_name(&me(), 7, NOW, 0, OsStr::new("a.md"));
        assert!(!purgeable(&name, &me(), 7, NOW));
        // A newer live session than the purge expected: still not an ended session's entry.
        assert!(!purgeable(
            &entry_name(&me(), 8, NOW, 0, OsStr::new("a.md")),
            &me(),
            7,
            NOW
        ));
        assert!(purgeable(&name, &me(), 8, NOW));
    }
    #[test]
    fn an_earlier_run_of_this_installation_has_ended() {
        let earlier = Owner {
            run: "00000000".into(),
            ..me()
        };
        let name = entry_name(&earlier, 9, NOW, 0, OsStr::new("a.md"));
        assert!(purgeable(&name, &me(), 0, NOW));
    }
    #[test]
    fn another_computers_entries_go_only_when_old() {
        let other = Owner {
            install: "fedcba9876543210".into(),
            ..me()
        };
        let name = |created| entry_name(&other, 0, created, 0, OsStr::new("a.md"));
        assert!(!purgeable(&name(NOW - FOREIGN_AGE_MS), &me(), 5, NOW));
        assert!(purgeable(&name(NOW - FOREIGN_AGE_MS - 1), &me(), 5, NOW));
        // Its clock ahead of this one.
        assert!(!purgeable(&name(NOW + 1000), &me(), 5, NOW));
    }
    #[test]
    fn names_docdock_did_not_make_stay() {
        for name in [
            "dropped here by hand.md",
            "123-4-5 a.md",
            "0123456789abcdef-89abcdef-7-1800000000000-3",
            "0123456789ABCDEF-89abcdef-7-1800000000000-3 a.md",
            "0123456789abcdef-89abcdef-7-x-3 a.md",
            "0123456789abcdef-89abcdef-7-1800000000000-3-4 a.md",
        ] {
            assert!(
                !purgeable(OsStr::new(name), &me(), u64::MAX, u64::MAX),
                "{name}"
            );
        }
    }
}
