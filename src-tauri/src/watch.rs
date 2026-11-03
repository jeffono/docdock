//! Watches the notes folder and tells the UI that something in it changed. The UI rereads the list and
//! the open note; nothing here decides what changed.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

pub const EVENT: &str = "notes-changed";

/// A burst of events, such as a save or a sync, becomes one notice after this much quiet.
const QUIET: Duration = Duration::from_millis(200);
/// A folder that never goes quiet still sends a notice this often.
const MAX_WAIT: Duration = Duration::from_secs(1);

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
        Err(error) => eprintln!("[watch] failed to watch {}: {error}", root.display()),
    }
}

/// Stops watching, so the next `ensure` starts again even for the same path: a folder that went away
/// and came back needs a new watch.
pub fn stop() {
    *WATCHING.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn start(app: AppHandle, root: &Path) -> notify::Result<RecommendedWatcher> {
    let (sender, events) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(sender)?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    let root = root.to_owned();
    thread::spawn(move || {
        while let Ok(first) = events.recv() {
            let mut changed = matters(&root, &first);
            let started = Instant::now();
            while started.elapsed() < MAX_WAIT {
                match events.recv_timeout(QUIET) {
                    Ok(event) => changed |= matters(&root, &event),
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            if changed && let Err(error) = app.emit_to("main", EVENT, ()) {
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
