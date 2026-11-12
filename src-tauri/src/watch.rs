//! Watches the notes folder and tells the UI that something in it changed. The UI rereads the list and
//! the open note; nothing here decides what changed.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

use crate::notes::Availability;

#[derive(Clone, Default, serde::Serialize)]
struct Changes {
    renames: Vec<(String, String)>,
}

pub const EVENT: &str = "notes-changed";

/// A burst of events, such as a save or a sync, becomes one notice after this much quiet.
const QUIET: Duration = Duration::from_millis(200);
/// A folder that never goes quiet still sends a notice this often.
const MAX_WAIT: Duration = Duration::from_secs(1);
/// How often the folder's availability is checked: a vault that locks or a drive that leaves may send
/// no event, and one that returns sends none to a watch that ended.
const POLL: Duration = Duration::from_secs(2);

/// The folder being watched. Dropping the watcher closes its channel, which ends its thread.
static WATCHING: Mutex<Option<(PathBuf, RecommendedWatcher)>> = Mutex::new(None);

/// Starts watching `root` unless it is already watched. A failure only means no live updates.
pub fn ensure(app: &AppHandle, root: &Path) {
    let mut watching = WATCHING.lock().unwrap_or_else(|e| e.into_inner());
    if watching.as_ref().is_some_and(|(path, _)| path == root) {
        return;
    }
    *watching = None;
    match start(app.clone(), root) {
        Ok(watcher) => *watching = Some((root.to_owned(), watcher)),
        // Watcher errors can carry paths; keep note names out of diagnostics.
        Err(_) => eprintln!("[watch] failed to watch the notes folder"),
    }
}

/// Tells the UI that the app itself changed the folder, as the watcher would.
pub fn notify(app: &AppHandle) {
    if let Err(error) = app.emit_to("main", EVENT, Changes::default()) {
        eprintln!("[watch] failed to notify the UI: {error}");
    }
}

/// Stops watching, so the next `ensure` starts again even for the same path: a folder that went away
/// and came back needs a new watch.
pub fn stop() {
    *WATCHING.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Tells the UI when the folder in use leaves or returns, so it forgets the notes or reopens them. On
/// leaving, the watch stops, so the next listing starts a new one, and the deleted notes kept for Undo
/// are forgotten. A different folder is the picker's change, which the UI already follows.
pub fn monitor(app: AppHandle) {
    thread::spawn(move || {
        let mut last = crate::notes::folder_state(&app);
        loop {
            thread::sleep(POLL);
            let now = crate::notes::folder_state(&app);
            let left_or_returned = matches!((&last, &now), (Some((a, was)), Some((b, is)))
                if a == b && was != is);
            if left_or_returned {
                if now
                    .as_ref()
                    .is_some_and(|(_, state)| *state != Availability::Available)
                {
                    stop();
                    crate::notes::forget_unavailable();
                }
                if let Err(error) = app.emit_to("main", EVENT, Changes::default()) {
                    eprintln!("[watch] failed to notify the UI: {error}");
                }
            }
            last = now;
        }
    });
}

fn start(app: AppHandle, root: &Path) -> notify::Result<RecommendedWatcher> {
    let (sender, events) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(sender)?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    let root = root.to_owned();
    thread::spawn(move || {
        let mut from: Option<(PathBuf, Option<usize>)> = None;
        while let Ok(first) = events.recv() {
            let mut changed = matters(&root, &first);
            let mut payload = Changes::default();
            collect_renames(&root, &first, &mut from, &mut payload);
            let started = Instant::now();
            while started.elapsed() < MAX_WAIT {
                match events.recv_timeout(QUIET) {
                    Ok(event) => {
                        changed |= matters(&root, &event);
                        collect_renames(&root, &event, &mut from, &mut payload);
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            if changed && let Err(error) = app.emit_to("main", EVENT, payload) {
                eprintln!("[watch] failed to notify the UI: {error}");
            }
        }
    });
    Ok(watcher)
}

/// A lost or unreadable event may hide a change, so it counts.
fn matters(root: &Path, event: &notify::Result<notify::Event>) -> bool {
    let Ok(event) = event else {
        return true;
    };
    !matches!(event.kind, EventKind::Access(_))
        && (event.paths.is_empty()
            || event
                .paths
                .iter()
                .any(|path| crate::notes::shown_change(root, path)))
}

fn collect_renames(
    root: &Path,
    event: &notify::Result<notify::Event>,
    from: &mut Option<(PathBuf, Option<usize>)>,
    changes: &mut Changes,
) {
    let Ok(event) = event else {
        *from = None;
        return;
    };
    let pair = match event.kind {
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() == 2 => {
            *from = None;
            Some((event.paths[0].clone(), event.paths[1].clone()))
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            *from = event
                .paths
                .first()
                .map(|p| (p.clone(), event.attrs.tracker()));
            None
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            from.take().and_then(|(old, tracker)| {
                (tracker == event.attrs.tracker())
                    .then(|| event.paths.first().map(|p| (old, p.clone())))
                    .flatten()
            })
        }
        _ => None,
    };
    if let Some((old, new)) = pair {
        rename_paths(root, &old, &new, changes);
    }
}
fn rename_paths(root: &Path, old: &Path, new: &Path, changes: &mut Changes) {
    if !old.starts_with(root)
        || !new.starts_with(root)
        || !crate::notes::shown_change(root, old)
        || !crate::notes::shown_change(root, new)
    {
        return;
    }
    if new.is_dir() {
        if let Ok(entries) = std::fs::read_dir(new) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| !kind.is_symlink()) {
                    rename_paths(root, &old.join(entry.file_name()), &entry.path(), changes);
                }
            }
        }
        return;
    }
    let relative = |path: &Path| {
        path.strip_prefix(root)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    };
    if old
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        && new
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        && let (Some(old), Some(new)) = (relative(old), relative(new))
    {
        changes.renames.push((old, new));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_markdown_rename_pairs_but_not_departures() {
        let root = Path::new(r"C:\notes");
        let mut pending = None;
        let mut changes = Changes::default();
        let event = |mode, paths: &[&str]| {
            let mut event = notify::Event::new(EventKind::Modify(ModifyKind::Name(mode)));
            for path in paths {
                event = event.add_path(root.join(path));
            }
            Ok(event)
        };
        collect_renames(
            root,
            &event(RenameMode::Both, &["a.md", "sub/b.md"]),
            &mut pending,
            &mut changes,
        );
        collect_renames(
            root,
            &event(RenameMode::From, &["sub/b.md"]),
            &mut pending,
            &mut changes,
        );
        collect_renames(
            root,
            &event(RenameMode::To, &["c.MD"]),
            &mut pending,
            &mut changes,
        );
        collect_renames(
            root,
            &event(RenameMode::Both, &["c.MD", "c.txt"]),
            &mut pending,
            &mut changes,
        );
        collect_renames(
            root,
            &event(RenameMode::Both, &["c.MD", r"C:\outside\c.md"]),
            &mut pending,
            &mut changes,
        );
        assert_eq!(
            changes.renames,
            vec![
                ("a.md".into(), "sub/b.md".into()),
                ("sub/b.md".into(), "c.MD".into())
            ]
        );
    }
}
