//! The notes folder's own trash, for deletes the Recycle Bin cannot take, such as on a mounted
//! Cryptomator vault or a network share. A deleted item moves to `.docdock-trash` at the root of the
//! notes folder, on the same volume, so it never leaves the folder (or the vault). Hidden folders are
//! skipped by listing, search, tags and the watcher.
//!
//! Entries are named `<install>-<run>-<session>-<created>-<n> <name>`, each field in lowercase base36:
//! the installation that made them (a random id kept in this computer's local app data), the run,
//! the panel session, the creation time in ms since the Unix epoch and a counter. A synced notes
//! folder may hold another computer's entries, and that computer's panel may still offer their Undo,
//! so the name says who may remove what:
//! - this run's entries go once their session has ended, never while it is the live one;
//! - another run of this installation (another Windows session of the same user, or an earlier run)
//!   is asked whether it still runs: its entries go once it does not;
//! - another installation's entries, and those of a run whose state cannot be told, go only after
//!   `OLD_MS`;
//! - a name in no such form was not made by DocDock and stays.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::OwnedHandle;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::platform;

pub const DIR: &str = ".docdock-trash";
/// Far longer than a panel stays open with an Undo, and than any plausible clock difference.
const OLD_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// Who makes entries: this installation and this run, in lowercase base36.
#[derive(Clone, Debug, PartialEq)]
pub struct Owner {
    install: String,
    run: String,
}
static OWNER: OnceLock<Owner> = OnceLock::new();
/// This run's presence, which other runs of the installation check before purging its entries.
static PRESENCE: OnceLock<Result<OwnedHandle, String>> = OnceLock::new();

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
fn is_base36(text: &str, max: usize) -> bool {
    (1..=max).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
}
/// A new installation id (40 random bits), to keep on this computer.
pub fn new_install_id() -> String {
    base36(random() >> 24)
}
/// Whether `id` is one `new_install_id` could have made.
pub fn valid_install_id(id: &str) -> bool {
    is_base36(id, 8)
}
/// Sets this installation's id, once, before the first entry; without it, an id for this run only.
pub fn set_install(install: String) {
    let _ = OWNER.set(Owner {
        install,
        run: base36(random() >> 32),
    });
}
fn owner() -> &'static Owner {
    OWNER.get_or_init(|| Owner {
        install: new_install_id(),
        run: base36(random() >> 32),
    })
}
fn presence_name(install: &str, run: &str) -> String {
    format!("DocDock-trash-{install}-{run}")
}
/// Announces this run, once, for as long as it runs. An entry is only made once that holds, so no
/// other run can take this one for ended.
fn announced() -> io::Result<()> {
    let me = owner();
    PRESENCE
        .get_or_init(|| {
            platform::presence::announce(&presence_name(&me.install, &me.run))
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map(|_| ())
        .map_err(|e| io::Error::other(format!("cannot announce this run: {e}")))
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The entry name for `name`, cut so the whole stays within `MAX_NAME`: never between the halves of
/// a surrogate pair, and without a trailing dot or space, which Windows drops from names. The cut
/// name only helps someone looking in the trash; Undo keeps the original path itself.
fn entry_name(owner: &Owner, session: u64, created: u64, n: u64, name: &OsStr) -> OsString {
    let prefix = format!(
        "{}-{}-{}-{}-{} ",
        owner.install,
        owner.run,
        base36(session),
        base36(created),
        base36(n)
    );
    let room = platform::MAX_NAME.saturating_sub(prefix.len());
    let mut wide: Vec<u16> = name.encode_wide().collect();
    if wide.len() > room {
        wide = platform::fit_name(name, room).encode_wide().collect();
        while wide
            .last()
            .is_some_and(|&u| u == u16::from(b'.') || u == u16::from(b' '))
        {
            wide.pop();
        }
    }
    let mut entry = OsString::from(prefix);
    entry.push(OsString::from_wide(&wide));
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
        is_base36(text, 13)
            .then(|| u64::from_str_radix(text, 36).ok())
            .flatten()
    };
    number(n)?;
    (is_base36(install, 8) && is_base36(run, 7)).then_some(Entry {
        install,
        run,
        session: number(session)?,
        created: number(created)?,
    })
}
/// Whether a purge by `me`, whose live panel session is `live`, removes the entry `name` at `now`
/// (ms since the Unix epoch). `running` tells whether another run of this installation still runs,
/// or None when that cannot be told. See the module comment.
fn purgeable(
    name: &OsStr,
    me: &Owner,
    live: u64,
    now: u64,
    running: impl Fn(&str, &str) -> Option<bool>,
) -> bool {
    let name = name.to_string_lossy();
    let Some(entry) = parse(&name) else {
        return false;
    };
    // A creation time ahead of this clock is not old.
    let old = now.saturating_sub(entry.created) > OLD_MS;
    if entry.install != me.install {
        return old;
    }
    if entry.run == me.run {
        return entry.session < live;
    }
    match running(entry.install, entry.run) {
        Some(running) => !running,
        None => old,
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Moves `target`, a note or a folder below `root`, into the trash as an entry of `session`, and
/// answers where it landed. A rename on the same volume: it either moves whole or stays. The trash
/// folder is created if needed and held, so a link planted under its name is refused rather than
/// followed out of the folder. The caller holds `target`'s ancestry.
pub fn put(root: &Path, target: &Path, session: u64) -> io::Result<PathBuf> {
    let name = target.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    announced()?;
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
    let running =
        |install: &str, run: &str| platform::presence::present(&presence_name(install, run)).ok();
    {
        let Ok(_held) = platform::hold_ancestry(&trash) else {
            return;
        };
        let Ok(entries) = fs::read_dir(&trash) else {
            return;
        };
        for entry in entries.flatten() {
            if !purgeable(&entry.file_name(), owner(), live(), now_ms(), running) {
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

    /// An entry name as an earlier run of this installation, no longer running, would make it.
    pub fn earlier_run_entry(name: &str) -> OsString {
        let earlier = Owner {
            install: owner().install.clone(),
            run: if owner().run == "0" { "1" } else { "0" }.into(),
        };
        entry_name(&earlier, 0, now_ms(), 0, OsStr::new(name))
    }
    fn me() -> Owner {
        Owner {
            install: "abc123".into(),
            run: "z9".into(),
        }
    }
    const NOW: u64 = 1_800_000_000_000;
    fn unknown(_: &str, _: &str) -> Option<bool> {
        None
    }

    #[test]
    fn entry_names_carry_their_owner_and_keep_the_original_name() {
        let name = entry_name(&me(), 7, NOW, 36, OsStr::new("a b.md"));
        assert_eq!(
            name.to_str(),
            Some(format!("abc123-z9-7-{}-10 a b.md", base36(NOW)).as_str())
        );
        let entry = parse(name.to_str().unwrap()).unwrap();
        assert_eq!((entry.session, entry.created), (7, NOW));
    }
    #[test]
    fn a_long_name_is_cut_to_fit_without_splitting_a_character() {
        // Two UTF-16 units each, so a cut at an odd place would split one.
        // The cut falls exactly between the halves of the first emoji.
        let room = platform::MAX_NAME - entry_name(&me(), 7, NOW, 0, OsStr::new("")).len();
        let long = format!("{}{}", "a".repeat(room - 1), "😀".repeat(3));
        let name = entry_name(&me(), 7, NOW, 0, OsStr::new(&long));
        assert!(name.encode_wide().count() <= platform::MAX_NAME);
        assert!(
            name.to_string_lossy().ends_with('a'),
            "a surrogate pair was split"
        );
    }
    #[test]
    fn a_cut_name_drops_the_dots_and_spaces_it_ends_with() {
        let room = platform::MAX_NAME - entry_name(&me(), 7, NOW, 0, OsStr::new("")).len();
        let dotted = format!("{}. .md", "a".repeat(room - 3));
        let cut = entry_name(&me(), 7, NOW, 0, OsStr::new(&dotted));
        assert!(cut.to_string_lossy().ends_with('a'), "{cut:?}");
    }
    #[test]
    fn this_runs_entries_go_only_once_their_session_has_ended() {
        let name = entry_name(&me(), 7, NOW, 0, OsStr::new("a.md"));
        assert!(!purgeable(&name, &me(), 7, NOW, unknown));
        // A newer live session than the purge expected: still not an ended session's entry.
        let newer = entry_name(&me(), 8, NOW, 0, OsStr::new("a.md"));
        assert!(!purgeable(&newer, &me(), 7, NOW, unknown));
        assert!(purgeable(&name, &me(), 8, NOW, unknown));
    }
    #[test]
    fn another_run_of_this_installation_goes_only_once_it_has_ended() {
        let other = Owner {
            run: "0".into(),
            ..me()
        };
        let name = entry_name(&other, 9, NOW, 0, OsStr::new("a.md"));
        assert!(!purgeable(&name, &me(), 0, NOW, |_, _| Some(true)));
        assert!(purgeable(&name, &me(), 0, NOW, |_, _| Some(false)));
        // Not knowing falls under the age rule.
        assert!(!purgeable(&name, &me(), 0, NOW + OLD_MS, unknown));
        assert!(purgeable(&name, &me(), 0, NOW + OLD_MS + 1, unknown));
    }
    #[test]
    fn another_computers_entries_go_only_when_old() {
        let other = Owner {
            install: "x".into(),
            ..me()
        };
        let gone = |created| {
            let name = entry_name(&other, 0, created, 0, OsStr::new("a.md"));
            purgeable(&name, &me(), 5, NOW, |_, _| Some(false))
        };
        assert!(!gone(NOW - OLD_MS));
        assert!(gone(NOW - OLD_MS - 1));
        // Its clock ahead of this one.
        assert!(!gone(NOW + 1000));
    }
    #[test]
    fn names_docdock_did_not_make_stay() {
        for name in [
            "dropped here by hand.md",
            "123-4-5 a.md",
            "abc123-z9-7-mz2xp0w0",
            "ABC123-z9-7-mz2xp0w0-0 a.md",
            "abc123-z9-7-mz2x_p0w0-0 a.md",
            "abc123-z9-7-mz2xp0w0-0-0 a.md",
            "abc1234567-z9-7-mz2xp0w0-0 a.md",
        ] {
            let gone = purgeable(OsStr::new(name), &me(), u64::MAX, u64::MAX, |_, _| {
                Some(false)
            });
            assert!(!gone, "{name}");
        }
    }
}
