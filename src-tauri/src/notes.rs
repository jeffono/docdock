//! Local note storage and validation.
use crate::dock::{Request, Side};
use crate::error::Error;
use crate::i18n::Locale;
use crate::pinned::{self, Kind};
use crate::platform;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    hash::{DefaultHasher, Hasher},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, UNIX_EPOCH},
};
use tauri::http::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue, X_CONTENT_TYPE_OPTIONS};
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

/// On Windows, images are at `http://note-asset.localhost/<relative path>`.
pub const IMAGE_SCHEME: &str = "note-asset";

/// A third-party note must not block the UI: conversion and inserting HTML into the DOM are costly.
const MAX_NOTE_BYTES: u64 = 2 * 1024 * 1024;

/// Extensions of the files a write creates next to its target. They are not `.md`, so `collect` skips
/// them.
const TEMP_SUFFIX: &str = "docdock-tmp";
const BACKUP_SUFFIX: &str = "docdock-bak";

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<PathBuf>,
    #[serde(default, deserialize_with = "side_or_default")]
    side: Side,
    /// Pinned targets shown as one line on this computer; the rest start as previews.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    collapsed: BTreeSet<String>,
}
/// An unknown side must not make the folder unreadable.
fn side_or_default<'de, D: Deserializer<'de>>(value: D) -> Result<Side, D::Error> {
    Ok(Side::deserialize(serde_json::Value::deserialize(value)?).unwrap_or_default())
}
#[derive(Serialize)]
pub struct Folder {
    path: String,
    available: bool,
}
#[derive(Serialize)]
pub struct Note {
    path: String,
    name: String,
}
#[derive(Serialize)]
pub struct OpenNote {
    text: String,
    version: String,
}
#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Saved {
    Saved { version: String },
    Conflict,
}
fn config(app: &AppHandle) -> Result<PathBuf, Error> {
    app.path()
        .app_config_dir()
        .map(|p| p.join("settings.json"))
        .map_err(Error::from)
}
/// Missing or corrupt settings read as the defaults.
fn load(path: &Path) -> Settings {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}
fn read_settings(path: &Path) -> Option<PathBuf> {
    load(path).folder
}
pub fn side(app: &AppHandle) -> Side {
    config(app).map(|p| load(&p).side).unwrap_or_default()
}
/// Held across load, change and save, so a folder change and a side change do not undo each other.
static SETTINGS: Mutex<()> = Mutex::new(());
fn update(app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Result<(), Error> {
    let path = config(app)?;
    let _guard = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    let mut settings = load(&path);
    change(&mut settings);
    save(&path, &settings)
}
pub fn set_side(app: &AppHandle, side: Side) -> Result<(), Error> {
    update(app, |settings| settings.side = side)
}
pub fn folder(app: &AppHandle) -> Result<Option<Folder>, Error> {
    Ok(read_settings(&config(app)?).map(|p| Folder {
        path: p.to_string_lossy().into_owned(),
        available: p.is_dir(),
    }))
}
fn save(path: &Path, settings: &Settings) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::unexpected("settings path has no folder"))?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec(settings).map_err(Error::unexpected)?;
    Ok(write_atomic(path, &bytes)?)
}
/// Writes a complete temporary file next to `path` and renames it over `path` in one step. Any failure
/// leaves the previous file intact. Only for the app's own settings; notes use `replace_note`.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = write_temp(path, bytes)?;
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}
fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}
/// Distinguishes the files of concurrent operations within the process; the process id distinguishes
/// processes.
static NEXT_SIBLING: AtomicU64 = AtomicU64::new(0);
/// An unused name next to `path`: `<name>.<pid>-<n>.<suffix>`. Leftovers of a crash are skipped, never
/// reused or deleted: they may hold the only copy of a version.
fn sibling(path: &Path, suffix: &str) -> io::Result<PathBuf> {
    let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    loop {
        let n = NEXT_SIBLING.fetch_add(1, Ordering::Relaxed);
        let mut unique = name.to_os_string();
        unique.push(format!(".{}-{n}.{suffix}", std::process::id()));
        let candidate = path.with_file_name(unique);
        match fs::symlink_metadata(&candidate) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(candidate),
            Err(e) => return Err(e),
            Ok(_) => {}
        }
    }
}
/// Creates a temporary file next to `path` holding `bytes`, synced to disk. On failure it is removed:
/// this operation created it.
fn write_temp(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    let temp = sibling(path, TEMP_SUFFIX)?;
    // `create_new` does not open through a link planted under the name.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    match written {
        Ok(()) => Ok(temp),
        Err(e) => {
            let _ = fs::remove_file(&temp);
            Err(e)
        }
    }
}
struct DialogGuard(Sender<Request>);
impl Drop for DialogGuard {
    fn drop(&mut self) {
        let _ = self.0.send(Request::SetDialog(false));
    }
}
pub fn choose(app: &AppHandle, requests: &Sender<Request>) -> Result<Option<Folder>, Error> {
    requests
        .send(Request::SetDialog(true))
        .map_err(Error::unexpected)?;
    let _guard = DialogGuard(requests.clone());
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| Error::unexpected("main window missing"))?;
    // Owned by the panel, the picker stays above it and does not open a second picker.
    let dialog = app.dialog().file().set_parent(&window);
    if let Some(path) = dialog.blocking_pick_folder() {
        let folder = Some(path.into_path().map_err(Error::unexpected)?);
        update(app, |settings| settings.folder = folder)?;
        crate::watch::stop();
    }
    folder(app)
}
fn root(app: &AppHandle) -> Result<PathBuf, Error> {
    let path = read_settings(&config(app)?).ok_or(Error::NoFolder)?;
    if !path.is_dir() {
        return Err(Error::FolderUnavailable);
    }
    Ok(path)
}
fn markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}
/// Whether a change at `path` can alter what the UI shows: not inside a hidden folder, which listing
/// skips, and not a temporary or backup file a write creates next to its target.
pub fn shown_change(root: &Path, path: &Path) -> bool {
    // Only folders below `root`, as listing does: the root may sit inside a hidden folder, and a file
    // such as `.ideas.md` is listed.
    let hidden = path.strip_prefix(root).is_ok_and(|relative| {
        relative.parent().is_some_and(|folders| {
            folders
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
    });
    let working = path
        .extension()
        .is_some_and(|e| e == TEMP_SUFFIX || e == BACKUP_SUFFIX);
    !hidden && !working
}
/// Visits every file below `dir`, skipping hidden folders. `is_dir` and `is_file` are false for
/// symlinks and junctions, so they are never followed.
fn walk(dir: &Path, visit: &mut impl FnMut(PathBuf)) -> Result<(), Error> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            if !entry.file_name().to_string_lossy().starts_with('.') {
                walk(&entry.path(), visit)?;
            }
        } else if ty.is_file() {
            visit(entry.path());
        }
    }
    Ok(())
}
/// The path the UI uses: relative to `root`, with `/`.
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
fn list(root: &Path) -> Result<Vec<Note>, Error> {
    let mut notes = Vec::new();
    walk(root, &mut |path| {
        if markdown(&path) {
            notes.push(Note {
                path: relative(root, &path),
                name: path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            });
        }
    })?;
    notes.sort_by(|a, b| {
        a.path
            .to_lowercase()
            .cmp(&b.path.to_lowercase())
            .then(a.path.cmp(&b.path))
    });
    Ok(notes)
}
/// Listing is what the UI does on opening a folder, so it also starts watching it.
pub fn list_for(app: &AppHandle) -> Result<Vec<Note>, Error> {
    let root = root(app)?;
    crate::watch::ensure(app, &root);
    list(&root)
}
/// `allowed` applies to both the request and canonical target: an 8.3 short name or symlink inside
/// the folder can change the requested extension to a different real extension.
fn resolve(root: &Path, relative: &str, allowed: fn(&Path) -> bool) -> Result<PathBuf, Error> {
    let path = Path::new(relative);
    if relative.contains(':')
        || relative.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || !allowed(path)
    {
        return Err(Error::InvalidPath);
    }
    let root = root.canonicalize()?;
    let target = root.join(path).canonicalize()?;
    if !target.starts_with(&root) || !target.is_file() || !allowed(&target) {
        return Err(Error::OutsideFolder);
    }
    Ok(target)
}
fn too_large(size: u64) -> Result<(), Error> {
    if size > MAX_NOTE_BYTES {
        return Err(Error::TooLarge {
            size_mb: size / 1_048_576,
            limit_mb: MAX_NOTE_BYTES / 1_048_576,
        });
    }
    Ok(())
}
fn read_bytes(path: &Path) -> Result<Vec<u8>, Error> {
    too_large(fs::metadata(path)?.len())?;
    Ok(fs::read(path)?)
}
fn read_note(path: &Path) -> Result<String, Error> {
    String::from_utf8(read_bytes(path)?).map_err(|_| Error::NotUtf8)
}
/// Identifies the exact bytes the editor loaded. Hashing the content, rather than using mtime and size,
/// also catches same-size edits and tools that preserve mtime. Tokens are never persisted, so the
/// unspecified `DefaultHasher` algorithm only needs to be stable within one run.
fn version(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    format!("{:016x}", hasher.finish())
}
pub fn open(app: &AppHandle, path: &str) -> Result<OpenNote, Error> {
    let text = read_note(&resolve(&root(app)?, path, markdown)?)?;
    Ok(OpenNote {
        version: version(text.as_bytes()),
        text,
    })
}
/// Serializes saves so the conflict check and the write are not interleaved within the app.
static SAVING: Mutex<()> = Mutex::new(());
/// Writes `content` only if the file still holds the version identified by `base`. The early check
/// avoids a write in the common cases; `replace_note` confirms it against the version actually replaced.
fn save_note_in(root: &Path, relative: &str, content: &str, base: &str) -> Result<Saved, Error> {
    too_large(content.len() as u64)?;
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let target = match resolve(root, relative, markdown) {
        Ok(target) if !base.is_empty() => target,
        Ok(_) => return Ok(Saved::Conflict),
        Err(_) if !exists(&root.join(relative)) => return recreate_note(root, relative, content),
        Err(error) => return Err(error),
    };
    let current = match read_bytes(&target) {
        Ok(current) => current,
        Err(_) if !exists(&target) => return recreate_note(root, relative, content),
        Err(error) => return Err(error),
    };
    if current == content.as_bytes() {
        return Ok(Saved::Saved {
            version: version(&current),
        });
    }
    if version(&current) != base {
        return Ok(Saved::Conflict);
    }
    replace_note(&target, content.as_bytes(), base)
}
/// Empty base expects absence; recreation never replaces a file that took the name.
fn recreate_note(root: &Path, relative: &str, content: &str) -> Result<Saved, Error> {
    let path = Path::new(relative);
    if relative.contains(':')
        || relative.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || !markdown(path)
    {
        return Err(Error::InvalidPath);
    }
    let root = root.canonicalize()?;
    let target = root.join(path);
    let parent = target.parent().ok_or(Error::InvalidPath)?;
    let mut held = Vec::new();
    // Reject links and prevent directory swaps during the write.
    for dir in parent.ancestors().collect::<Vec<_>>().into_iter().rev() {
        held.push(platform::hold_directory(dir)?);
    }
    if !parent.canonicalize()?.starts_with(&root) {
        return Err(Error::OutsideFolder);
    }
    if exists(&target) {
        return Ok(Saved::Conflict);
    }
    let temp = write_temp(&target, content.as_bytes())?;
    let result = match platform::move_new(&temp, &target) {
        Ok(()) => Ok(Saved::Saved {
            version: version(content.as_bytes()),
        }),
        Err(_) if exists(&target) => Ok(Saved::Conflict),
        Err(error) => Err(error.into()),
    };
    let _ = fs::remove_file(&temp);
    result
}
/// Replaces `target` with `content` through `ReplaceFileW`, which moves the replaced file to a backup:
/// those are exactly the bytes that were overwritten, even if another app wrote after the check above.
fn replace_note(target: &Path, content: &[u8], base: &str) -> Result<Saved, Error> {
    let fail = |e: io::Error| Error::SaveFailed {
        detail: e.to_string(),
    };
    let backup = sibling(target, BACKUP_SUFFIX).map_err(fail)?;
    let temp = write_temp(target, content).map_err(fail)?;
    if let Err(e) = replace(target, &temp, &backup) {
        // Still ours: the target never took it.
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    settle(target, &backup, content, base)
}
/// `ReplaceFileW` with its failures undone. `ERROR_UNABLE_TO_REMOVE_REPLACED` and
/// `ERROR_UNABLE_TO_MOVE_REPLACEMENT` (with a backup name) leave every file under its own name;
/// `ERROR_UNABLE_TO_MOVE_REPLACEMENT_2` leaves `target` under the `backup` name, so it is moved back.
/// Checking the names instead of the code also covers undocumented failures.
fn replace(target: &Path, replacement: &Path, backup: &Path) -> Result<(), Error> {
    let Err(error) = platform::replace_file(target, replacement, backup) else {
        return Ok(());
    };
    // `move_new` refuses to overwrite: if something took the name meanwhile, both files stay.
    if exists(backup)
        && let Err(undo) = platform::move_new(backup, target)
    {
        return Err(Error::PreviousKept {
            path: display(backup),
            detail: format!("{error}; {undo}"),
        });
    }
    Err(Error::SaveFailed {
        detail: error.to_string(),
    })
}
fn display(path: &Path) -> String {
    path.display().to_string()
}
/// Decides the save after the replace: `target` holds `content` and `backup` the replaced file. If the
/// backup is not `base`, an external write slipped in: it goes back to `target`, our copy is discarded
/// (the editor keeps the text) and the answer is a conflict. A version is deleted only once an identical
/// copy is at `target` or, for `content`, in the editor; anything else stays on disk and is reported.
fn settle(target: &Path, backup: &Path, content: &[u8], base: &str) -> Result<Saved, Error> {
    if discard_if(backup, |old| old == content || version(old) == base) {
        return Ok(Saved::Saved {
            version: version(content),
        });
    }
    let ours = sibling(target, BACKUP_SUFFIX).map_err(|e| Error::ExternalKept {
        path: display(backup),
        detail: e.to_string(),
    })?;
    swap_back(target, backup, &ours)?;
    if discard_if(&ours, |copy| copy == content) {
        return Ok(Saved::Conflict);
    }
    Err(Error::OursKept {
        path: display(&ours),
    })
}
/// Moves `target` aside to `ours` and `backup` back to `target`. The backup is the original file itself,
/// so its identity returns with it, and renaming works while another app still has it open, where
/// `ReplaceFileW` fails with a sharing violation. The name is absent for an instant between the renames.
fn swap_back(target: &Path, backup: &Path, ours: &Path) -> Result<(), Error> {
    let external = |error: io::Error| Error::ExternalKept {
        path: display(backup),
        detail: error.to_string(),
    };
    platform::move_new(target, ours).map_err(external)?;
    platform::move_new(backup, target).map_err(|error| match platform::move_new(ours, target) {
        Ok(()) => external(error),
        Err(undo) => Error::ExternalAndOursKept {
            external: display(backup),
            ours: display(ours),
            detail: format!("{error}; {undo}"),
        },
    })
}
/// Deletes `path` only if `matches` accepts its bytes, read through a handle no other process shares,
/// so nothing writes between the check and the delete. A file still open elsewhere may still change, so
/// it is kept; a few short retries let a scanner or sync client close it first.
fn discard_if(path: &Path, matches: impl Fn(&[u8]) -> bool) -> bool {
    for _ in 0..5 {
        match platform::open_exclusive(path) {
            Ok(Some(mut file)) => {
                let mut bytes = Vec::new();
                return file.read_to_end(&mut bytes).is_ok()
                    && matches(&bytes)
                    && platform::delete_on_close(&file).is_ok();
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => return false,
        }
    }
    false
}
pub fn save_note(app: &AppHandle, path: &str, content: &str, base: &str) -> Result<Saved, Error> {
    save_note_in(&root(app)?, path, content, base)
}
/// The pinned items live in this note at the root of the folder (decision 0005).
const PINNED: &str = "_pinned.md";
/// A read, a line edit and a write that keep losing to external writes give up after this many tries.
const PINNED_TRIES: usize = 3;

#[derive(Serialize)]
pub struct PinnedItem {
    #[serde(flatten)]
    item: pinned::Item,
    /// A note that no longer exists at that path; the UI offers to remove the item.
    missing: bool,
    /// Shown as one line on this computer.
    collapsed: bool,
}
#[derive(Serialize)]
pub struct Pinned {
    items: Vec<PinnedItem>,
    /// `_pinned.md` is gone but a copy of its last read version can bring it back.
    restorable: bool,
}
/// The last version of `_pinned.md` read in `folder`, kept in `app_config_dir` so a deleted file can come
/// back. One slot: it follows the folder in use.
#[derive(Serialize, Deserialize)]
struct PinnedCopy {
    folder: PathBuf,
    text: String,
}
fn copy_path(app: &AppHandle) -> Result<PathBuf, Error> {
    Ok(config(app)?.with_file_name("pinned-copy.json"))
}
fn read_copy(store: &Path) -> Option<PinnedCopy> {
    serde_json::from_slice(&fs::read(store).ok()?).ok()
}
/// The copy for `root`, if it holds anything worth restoring.
fn restorable_copy(store: &Path, root: &Path) -> Option<String> {
    read_copy(store)
        .filter(|copy| copy.folder == root && !copy.text.trim().is_empty())
        .map(|copy| copy.text)
}
/// Best effort: a copy that cannot be written only means a deleted file cannot be restored.
fn keep_copy(store: &Path, root: &Path, text: &str) {
    if read_copy(store).is_some_and(|copy| copy.folder == root && copy.text == text) {
        return;
    }
    let copy = PinnedCopy {
        folder: root.to_owned(),
        text: text.to_owned(),
    };
    let written = store
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| write_atomic(store, &serde_json::to_vec(&copy)?));
    if let Err(error) = written {
        eprintln!("[pinned] failed to keep a copy: {error}");
    }
}
/// The text and version of `_pinned.md`, or `None` if it does not exist.
fn read_pinned(root: &Path) -> Result<Option<(String, String)>, Error> {
    if !exists(&root.join(PINNED)) {
        return Ok(None);
    }
    let text = read_note(&resolve(root, PINNED, markdown)?)?;
    let version = version(text.as_bytes());
    Ok(Some((text, version)))
}
fn pinned_state(root: &Path, store: &Path, collapsed: &BTreeSet<String>) -> Result<Pinned, Error> {
    let Some((text, _)) = read_pinned(root)? else {
        return Ok(Pinned {
            items: Vec::new(),
            restorable: restorable_copy(store, root).is_some(),
        });
    };
    keep_copy(store, root, &text);
    let items = pinned::parse(&text)
        .into_iter()
        .map(|item| PinnedItem {
            missing: item.kind == Kind::Note && resolve(root, &item.target, markdown).is_err(),
            collapsed: collapsed.contains(&item.target),
            item,
        })
        .collect();
    Ok(Pinned {
        items,
        restorable: false,
    })
}
/// Applies a line edit to the current `_pinned.md` and saves it against the version it was read at. If
/// the file changed in between, it is read again and the edit applied to the new text; after
/// `PINNED_TRIES` such losses the edit is abandoned. A missing file is created.
fn edit_pinned(root: &Path, edit: impl Fn(&str) -> String) -> Result<(), Error> {
    for _ in 0..PINNED_TRIES {
        let (text, base) = read_pinned(root)?.unwrap_or_default();
        let edited = edit(&text);
        if edited == text {
            return Ok(());
        }
        if let Saved::Saved { .. } = save_note_in(root, PINNED, &edited, &base)? {
            return Ok(());
        }
    }
    Err(Error::PinnedChanged)
}
/// Brings back the last version read, only if `_pinned.md` is absent: an empty base never replaces.
fn restore_pinned_in(root: &Path, store: &Path) -> Result<(), Error> {
    if let Some(text) = restorable_copy(store, root) {
        save_note_in(root, PINNED, &text, "")?;
    }
    Ok(())
}
fn collapsed(app: &AppHandle) -> Result<BTreeSet<String>, Error> {
    Ok(load(&config(app)?).collapsed)
}
pub fn pinned(app: &AppHandle) -> Result<Pinned, Error> {
    pinned_state(&root(app)?, &copy_path(app)?, &collapsed(app)?)
}
/// `target` is a note's path (it must exist) or an http(s) URL. `title` is the link text.
pub fn pin(app: &AppHandle, target: &str, title: &str) -> Result<Pinned, Error> {
    let root = root(app)?;
    let (kind, target) = match pinned_kind(target) {
        Kind::Url => {
            let url = tauri::Url::parse(target).map_err(|e| Error::InvalidUrl {
                detail: e.to_string(),
            })?;
            (Kind::Url, url.to_string())
        }
        Kind::Note => {
            resolve(&root, target, markdown)?;
            (Kind::Note, target.to_owned())
        }
    };
    edit_pinned(&root, |text| pinned::pin(text, kind, &target, title))?;
    pinned(app)
}
fn pinned_kind(target: &str) -> Kind {
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Kind::Url
    } else {
        Kind::Note
    }
}
/// Works for a missing note too: that is what "remove" is for.
pub fn unpin(app: &AppHandle, target: &str) -> Result<Pinned, Error> {
    edit_pinned(&root(app)?, |text| pinned::unpin(text, target))?;
    update(app, |settings| {
        settings.collapsed.remove(target);
    })?;
    pinned(app)
}
pub fn move_pinned(app: &AppHandle, target: &str, to: usize) -> Result<Pinned, Error> {
    edit_pinned(&root(app)?, |text| pinned::move_item(text, target, to))?;
    pinned(app)
}
pub fn set_collapsed(app: &AppHandle, target: &str, collapsed: bool) -> Result<Pinned, Error> {
    update(app, |settings| {
        if collapsed {
            settings.collapsed.insert(target.to_owned());
        } else {
            settings.collapsed.remove(target);
        }
    })?;
    pinned(app)
}
pub fn restore_pinned(app: &AppHandle) -> Result<Pinned, Error> {
    restore_pinned_in(&root(app)?, &copy_path(app)?)?;
    pinned(app)
}
/// Bound IPC writes and disk use for accidental large clipboard images (10 MiB).
const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;
fn attachment_extension(bytes: &[u8]) -> Result<&'static str, Error> {
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(Error::ImageTooLarge { limit_mb: 10 });
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok("png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Ok("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Ok("gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Ok("webp")
    } else {
        Err(Error::UnsupportedImage)
    }
}
fn paste_image_in(root: &Path, path: &str, bytes: &[u8], stamp: &str) -> Result<String, Error> {
    let ext = attachment_extension(bytes)?;
    let note = resolve(root, path, markdown)?;
    let parent = note.parent().ok_or(Error::InvalidPath)?;
    if root
        .join(path)
        .parent()
        .ok_or(Error::InvalidPath)?
        .canonicalize()?
        != parent
    {
        return Err(Error::AttachmentFolderUnsafe);
    }
    // Lock canonical ancestry so no component can be swapped for a junction during the write.
    let mut held = Vec::new();
    for dir in parent.ancestors().collect::<Vec<_>>().into_iter().rev() {
        held.push(platform::hold_directory(dir).map_err(|_| Error::AttachmentFolderUnsafe)?);
    }
    // Revalidate after locking: resolve may have raced with an ancestor rename.
    if resolve(root, path, markdown)? != note {
        return Err(Error::AttachmentFolderUnsafe);
    }
    let folder = parent.join("attachments");
    match fs::create_dir(&folder) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    held.push(platform::hold_directory(&folder).map_err(|_| Error::AttachmentFolderUnsafe)?);
    let temp = write_temp(&folder.join("pasted"), bytes)?;
    let result = (|| {
        for n in 1u64.. {
            let suffix = if n == 1 {
                String::new()
            } else {
                format!("-{n}")
            };
            let name = format!("pasted-{stamp}{suffix}.{ext}");
            let target = folder.join(&name);
            match platform::move_new(&temp, &target) {
                Ok(()) => return Ok(format!("attachments/{name}")),
                Err(_) if exists(&target) => continue,
                Err(e) => return Err(e.into()),
            }
        }
        unreachable!()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
pub fn paste_image(app: &AppHandle, path: &str, bytes: &[u8]) -> Result<String, Error> {
    paste_image_in(&root(app)?, path, bytes, &platform::paste_timestamp())
}
/// A backup a save left behind, for example when the app closed between the two renames that put an
/// external version back.
#[derive(Serialize, Debug, PartialEq)]
pub struct Leftover {
    /// The backup itself.
    path: String,
    /// The note it came from, which may no longer exist.
    note: String,
    /// Last write, in milliseconds since the Unix epoch; 0 if unknown.
    modified: u64,
}
/// The note a backup came from: `<note>.<pid>-<n>.docdock-bak`, where `<note>` is a `.md` file.
fn leftover_note(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let (note, id) = name
        .strip_suffix(BACKUP_SUFFIX)?
        .strip_suffix('.')?
        .rsplit_once('.')?;
    let (pid, n) = id.split_once('-')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let note = path.with_file_name(note);
    (digits(pid) && digits(n) && markdown(&note)).then_some(note)
}
fn is_leftover(path: &Path) -> bool {
    leftover_note(path).is_some()
}
fn modified(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64)
}
/// Taken under the save lock: a save in progress owns its backup for a moment, and that is not a
/// leftover.
fn leftovers_in(root: &Path) -> Result<Vec<Leftover>, Error> {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let mut found = Vec::new();
    walk(root, &mut |path| {
        if let Some(note) = leftover_note(&path) {
            found.push(Leftover {
                path: relative(root, &path),
                note: relative(root, &note),
                modified: modified(&path),
            });
        }
    })?;
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}
pub fn leftovers(app: &AppHandle) -> Result<Vec<Leftover>, Error> {
    leftovers_in(&root(app)?)
}
/// Renames a leftover to its note's name or, if that is taken, to `<stem> (<label>).md`,
/// `<stem> (<label> 2).md` and so on. Never overwrites and never deletes. Returns the new note's path.
fn restore_in(root: &Path, leftover: &str, label: &str) -> Result<String, Error> {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let root = root.canonicalize()?;
    let source = resolve(&root, leftover, is_leftover)?;
    let note = leftover_note(&source).ok_or(Error::InvalidPath)?;
    let stem = note.file_stem().unwrap_or_default();
    for n in 1..=100 {
        let target = match n {
            1 => note.clone(),
            _ => {
                let mut name = stem.to_os_string();
                name.push(match n {
                    2 => format!(" ({label}).md"),
                    _ => format!(" ({label} {}).md", n - 1),
                });
                note.with_file_name(name)
            }
        };
        match platform::move_new(&source, &target) {
            Ok(()) => return Ok(relative(&root, &target)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::unexpected("no free name to restore the note"))
}
pub fn restore(app: &AppHandle, leftover: &str) -> Result<String, Error> {
    let label = match *app.state::<Locale>() {
        Locale::En => "recovered",
        Locale::PtBr => "recuperada",
    };
    restore_in(&root(app)?, leftover, label)
}
/// SVG is excluded because it can contain scripts.
fn image_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}
fn is_image(path: &Path) -> bool {
    image_type(path).is_some()
}
fn read_image(root: &Path, uri_path: &str) -> Option<(Vec<u8>, &'static str)> {
    let relative = percent_decode_str(uri_path.strip_prefix('/')?)
        .decode_utf8()
        .ok()?;
    let target = resolve(root, &relative, is_image).ok()?;
    let content_type = image_type(&target)?;
    Some((fs::read(target).ok()?, content_type))
}
fn empty_response(status: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = status;
    response
}
/// The `web` window also has the protocol, but its content is remote. `no-store` prevents the
/// WebView2 profile shared between windows from caching the image.
fn image_response(webview: &str, root: Option<PathBuf>, uri_path: &str) -> Response<Vec<u8>> {
    if webview != "main" {
        return empty_response(StatusCode::FORBIDDEN);
    }
    let Some((body, content_type)) = root.and_then(|root| read_image(&root, uri_path)) else {
        return empty_response(StatusCode::NOT_FOUND);
    };
    let mut response = Response::new(body);
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
pub fn image_for(app: &AppHandle, webview: &str, uri_path: &str) -> Response<Vec<u8>> {
    image_response(webview, root(app).ok(), uri_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "docdock-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn mklink(flags: &[&str], link: &Path, target: &Path) -> bool {
        std::process::Command::new("cmd")
            .args(["/C", "mklink"])
            .args(flags)
            .arg(link)
            .arg(target)
            .output()
            .unwrap()
            .status
            .success()
    }
    #[test]
    fn list_filters_and_sorts() {
        let t = Temp::new();
        fs::create_dir(t.0.join(".oculta")).unwrap();
        fs::create_dir(t.0.join("sub")).unwrap();
        for p in ["z.MD", "A.md", "x.txt", ".oculta/segredo.md", "sub/b.md"] {
            fs::write(t.0.join(p), "").unwrap();
        }
        fs::write(t.0.join(".ponto.md"), "").unwrap();
        let paths: Vec<_> = list(&t.0).unwrap().into_iter().map(|n| n.path).collect();
        assert_eq!(paths, [".ponto.md", "A.md", "sub/b.md", "z.MD"]);
        // The watcher reports changes to what is listed and nothing inside hidden folders.
        let shown = |p: &str| shown_change(&t.0, &t.0.join(p));
        assert!(shown(".ponto.md") && shown("sub/b.md") && shown("sub"));
        assert!(!shown(".oculta/segredo.md") && !shown("a.md.1-2.docdock-tmp"));
    }
    #[test]
    fn rejects_unsafe_paths() {
        let t = Temp::new();
        fs::write(t.0.join("ok.md"), "ok").unwrap();
        assert!(resolve(&t.0, "ok.md", markdown).is_ok());
        for p in [
            "../ok.md",
            "/ok.md",
            "C:/ok.md",
            "//servidor/ok.md",
            "ok.md:oculto",
            "ok.txt",
            "sub\\ok.md",
        ] {
            assert!(resolve(&t.0, p, markdown).is_err(), "{p}");
        }
    }
    #[test]
    fn does_not_follow_junction_outside_folder() {
        let t = Temp::new();
        let (notas, fora) = (t.0.join("notas"), t.0.join("fora"));
        fs::create_dir(&notas).unwrap();
        fs::create_dir(&fora).unwrap();
        fs::write(fora.join("segredo.md"), "x").unwrap();
        assert!(mklink(&["/J"], &notas.join("atalho"), &fora));
        assert!(list(&notas).unwrap().is_empty());
        assert!(resolve(&notas, "atalho/segredo.md", markdown).is_err());
    }
    #[test]
    fn pasted_images_accept_only_known_formats() {
        let t = Temp::new();
        fs::create_dir(t.0.join("sub")).unwrap();
        fs::write(t.0.join("sub/a.md"), "unchanged").unwrap();
        let png = b"\x89PNG\r\n\x1a\n";
        let paste = |bytes: &[u8]| paste_image_in(&t.0, "sub/a.md", bytes, "20261103-123456");
        for (bytes, ext) in [
            (png.as_slice(), "png"),
            (b"\xff\xd8\xff", "jpg"),
            (b"GIF87a", "gif"),
            (b"GIF89a", "gif"),
            (b"RIFF1234WEBP", "webp"),
        ] {
            assert_eq!(attachment_extension(bytes), Ok(ext));
        }
        for bytes in [
            b"<svg/>".as_slice(),
            b"BM1234",
            b"RIFF",
            b"",
            b"not an image",
        ] {
            assert_eq!(paste(bytes), Err(Error::UnsupportedImage));
        }
        assert_eq!(
            paste(&vec![0; MAX_ATTACHMENT_BYTES + 1]),
            Err(Error::ImageTooLarge { limit_mb: 10 })
        );
        assert!(paste_image_in(&t.0, "../a.md", png, "stamp").is_err());
        assert!(!t.0.join("sub/attachments").exists());
        assert_eq!(
            fs::read_to_string(t.0.join("sub/a.md")).unwrap(),
            "unchanged"
        );
    }
    #[test]
    fn pasted_images_never_overwrite() {
        let t = Temp::new();
        fs::create_dir(t.0.join("sub")).unwrap();
        fs::write(t.0.join("sub/a.md"), "unchanged").unwrap();
        let png = b"\x89PNG\r\n\x1a\n";
        let paste = |bytes: &[u8]| paste_image_in(&t.0, "sub/a.md", bytes, "20261103-123456");
        let first = paste(png).unwrap();
        assert_eq!(first, "attachments/pasted-20261103-123456.png");
        let second = paste(b"\x89PNG\r\n\x1a\nsecond").unwrap();
        assert_eq!(second, "attachments/pasted-20261103-123456-2.png");
        assert_eq!(fs::read(t.0.join("sub").join(&first)).unwrap(), png);
        assert_eq!(
            fs::read_dir(t.0.join("sub/attachments")).unwrap().count(),
            2
        );
        let response = image_response(
            "main",
            Some(t.0.clone()),
            "/sub/attachments/pasted-20261103-123456.png",
        );
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body(), png);
        assert_eq!(
            fs::read_to_string(t.0.join("sub/a.md")).unwrap(),
            "unchanged"
        );
    }
    #[test]
    fn attachments_reject_junctions() {
        let t = Temp::new();
        fs::create_dir(t.0.join("notes")).unwrap();
        fs::create_dir(t.0.join("outside")).unwrap();
        fs::write(t.0.join("notes/a.md"), "").unwrap();
        assert!(mklink(
            &["/J"],
            &t.0.join("notes").join("attachments"),
            &t.0.join("outside")
        ));
        assert_eq!(
            paste_image_in(&t.0.join("notes"), "a.md", b"GIF89a", "stamp"),
            Err(Error::AttachmentFolderUnsafe)
        );
        assert_eq!(fs::read_dir(t.0.join("outside")).unwrap().count(), 0);
    }
    #[test]
    fn held_directory_cannot_be_renamed_and_refuses_junctions() {
        let t = Temp::new();
        fs::create_dir(t.0.join("notes")).unwrap();
        let dir = t.0.join("outside");
        fs::create_dir(&dir).unwrap();
        let held = platform::hold_directory(&dir).unwrap();
        assert!(fs::rename(&dir, t.0.join("renamed")).is_err());
        drop(held);
        fs::rename(&dir, t.0.join("renamed")).unwrap();
        // Junctions into the notes folder are refused too.
        assert!(mklink(
            &["/J"],
            &t.0.join("inside-link"),
            &t.0.join("notes")
        ));
        assert!(platform::hold_directory(&t.0.join("inside-link")).is_err());
    }
    #[test]
    fn image_only_leaves_folder_for_main_window() {
        let t = Temp::new();
        let (notas, fora) = (t.0.join("notas"), t.0.join("fora"));
        fs::create_dir_all(notas.join("com espaco")).unwrap();
        fs::create_dir(&fora).unwrap();
        fs::write(fora.join("segredo.png"), "x").unwrap();
        for p in ["com espaco/foto.PNG", "nota.md", "vetor.svg", "fragil.SVG"] {
            fs::write(notas.join(p), "imagem").unwrap();
        }
        assert!(mklink(&["/J"], &notas.join("atalho"), &fora));
        let pedir = |webview, uri| image_response(webview, Some(notas.clone()), uri);

        let ok = pedir("main", "/com%20espaco/foto.PNG");
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.body(), b"imagem");
        assert_eq!(ok.headers()[CONTENT_TYPE], "image/png");
        assert_eq!(ok.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");

        let web = pedir("web", "/com%20espaco/foto.PNG");
        assert_eq!(web.status(), StatusCode::FORBIDDEN);
        assert!(web.body().is_empty());

        for uri in [
            "/../fora/segredo.png",
            "/%2e%2e/fora/segredo.png",
            "/..%2ffora%2fsegredo.png",
            "/..%5cfora%5csegredo.png",
            "/atalho/segredo.png",
            "/vetor.svg",
            "/fragil.SVG",
            "/nota.md",
            "/com%20espaco/foto.PNG%3Aoculto",
            "/%FF.png",
            "/",
        ] {
            assert_eq!(pedir("main", uri).status(), StatusCode::NOT_FOUND, "{uri}");
        }
        let sem_pasta = image_response("main", None, "/com%20espaco/foto.PNG");
        assert_eq!(sem_pasta.status(), StatusCode::NOT_FOUND);

        // File symlinks require Windows developer mode; without it, this case does not apply.
        if mklink(&[], &notas.join("falso.png"), &notas.join("nota.md")) {
            assert_eq!(pedir("main", "/falso.png").status(), StatusCode::NOT_FOUND);
        }
    }
    #[test]
    fn oversized_notes_are_neither_read_nor_saved() {
        let t = Temp::new();
        let p = t.0.join("grande.md");
        fs::write(&p, vec![b'x'; MAX_NOTE_BYTES as usize]).unwrap();
        assert!(read_note(&p).is_ok());
        let grande = "x".repeat(MAX_NOTE_BYTES as usize + 1);
        assert!(save_note_in(&t.0, "grande.md", &grande, "").is_err());
        fs::write(&p, &grande).unwrap();
        assert!(read_note(&p).is_err());
    }
    #[test]
    fn recreating_a_deleted_note_never_overwrites_a_taken_name() {
        let t = Temp::new();
        let path = t.0.join("a.md");
        let text = "mine\r\n";
        assert!(matches!(
            save_note_in(&t.0, "a.md", text, "old"),
            Ok(Saved::Saved { .. })
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        fs::write(&path, "external").unwrap();
        assert_eq!(recreate_note(&t.0, "a.md", text), Ok(Saved::Conflict));
        assert_eq!(save_note_in(&t.0, "a.md", text, ""), Ok(Saved::Conflict));
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    }
    #[test]
    fn save_never_writes_outside_folder() {
        let t = Temp::new();
        let (notas, fora) = (t.0.join("notas"), t.0.join("fora.md"));
        fs::create_dir(&notas).unwrap();
        fs::write(&fora, "fora").unwrap();
        assert!(save_note_in(&notas, "../fora.md", "x", &version(b"fora")).is_err());
        assert_eq!(fs::read_to_string(&fora).unwrap(), "fora");
        assert!(save_note_in(&notas, "../escape.md", "x", "").is_err());
        assert!(save_note_in(&notas, "missing/a.md", "x", "").is_err());
        fs::create_dir(t.0.join("real")).unwrap();
        assert!(mklink(&["/J"], &notas.join("link"), &t.0.join("real")));
        assert!(save_note_in(&notas, "link/a.md", "x", "").is_err());
        assert!(!t.0.join("real/a.md").exists());
        // Junctions are refused even when they point back inside the folder.
        fs::create_dir(notas.join("real")).unwrap();
        assert!(mklink(&["/J"], &notas.join("inner"), &notas.join("real")));
        assert!(save_note_in(&notas, "inner/a.md", "x", "").is_err());
        assert!(!notas.join("real/a.md").exists());
        assert!(!t.0.join("escape.md").exists());
    }
    #[test]
    fn save_refuses_external_changes() {
        let t = Temp::new();
        let notas = t.0.join("notas");
        fs::create_dir(&notas).unwrap();
        let p = notas.join("a.md");
        fs::write(&p, "um\r\n").unwrap();
        let base = version(b"um\r\n");
        fs::write(&p, "externo").unwrap();
        let conflict = save_note_in(&notas, "a.md", "meu", &base);
        assert_eq!(conflict, Ok(Saved::Conflict));
        assert_eq!(fs::read_to_string(&p).unwrap(), "externo");

        let saved = save_note_in(&notas, "a.md", "dois\r\n", &version(b"externo"));
        let version_dois = version(b"dois\r\n");
        assert_eq!(fs::read_to_string(&p).unwrap(), "dois\r\n");
        assert_eq!(
            saved,
            Ok(Saved::Saved {
                version: version_dois.clone()
            })
        );

        // An external write between the check and the replace ends up in the backup: it goes back to
        // the note, our copy is discarded and nothing else is left.
        let backup = notas.join("a.md.0-0.docdock-bak");
        fs::write(&backup, "externo2").unwrap();
        let conflict = settle(&p, &backup, b"dois\r\n", &version(b"externo"));
        assert_eq!(conflict, Ok(Saved::Conflict));
        assert_eq!(fs::read_to_string(&p).unwrap(), "externo2");
        assert_eq!(fs::read_dir(&notas).unwrap().count(), 1);
        fs::write(&backup, "externo2").unwrap();
        let saved = settle(&p, &backup, b"tres", &version(b"externo2"));
        assert!(matches!(saved, Ok(Saved::Saved { .. })));
        assert_eq!(fs::read_dir(&notas).unwrap().count(), 1);
    }
    #[test]
    fn failed_save_keeps_original_and_removes_temp() {
        use std::os::windows::fs::OpenOptionsExt;
        let t = Temp::new();
        let p = t.0.join("a.md");
        fs::write(&p, "original").unwrap();
        // A temp left by a crash is never deleted, reused or listed as a note.
        let leftover =
            t.0.join(format!("a.md.{}-0.{TEMP_SUFFIX}", std::process::id()));
        fs::write(&leftover, "novo").unwrap();
        // Another app holding the note without delete sharing makes the replace fail.
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1) // FILE_SHARE_READ
            .open(&p)
            .unwrap();
        let error = save_note_in(&t.0, "a.md", "novo", &version(b"original")).unwrap_err();
        assert!(matches!(error, Error::SaveFailed { .. }), "{error:?}");
        drop(lock);
        assert_eq!(fs::read_to_string(&p).unwrap(), "original");
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 2);
        assert_eq!(fs::read_to_string(&leftover).unwrap(), "novo");
        let paths: Vec<_> = list(&t.0).unwrap().into_iter().map(|n| n.path).collect();
        assert_eq!(paths, ["a.md"]);
    }
    /// A notes folder and a separate place for the copy of `_pinned.md`.
    fn pinned_setup(t: &Temp) -> (PathBuf, PathBuf) {
        let root = t.0.join("notes");
        fs::create_dir(&root).unwrap();
        (root, t.0.join("config").join("pinned-copy.json"))
    }
    fn no_collapsed() -> BTreeSet<String> {
        BTreeSet::new()
    }
    #[test]
    fn pinning_creates_a_missing_file() {
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        edit_pinned(&root, |text| pinned::pin(text, Kind::Note, "a b.md", "A")).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(PINNED)).unwrap(),
            "- [A](a%20b.md)\n"
        );
    }
    #[test]
    fn external_change_between_read_and_write_is_kept() {
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        let path = root.join(PINNED);
        fs::write(&path, "- [A](a.md)\r\n").unwrap();
        let calls = std::cell::Cell::new(0);
        edit_pinned(&root, |text| {
            if calls.replace(calls.get() + 1) == 0 {
                fs::write(&path, "# Mine\r\n- [A](a.md)\r\n- [X](x.md)\r\n").unwrap();
            }
            pinned::pin(text, Kind::Note, "b.md", "B")
        })
        .unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# Mine\r\n- [A](a.md)\r\n- [X](x.md)\r\n- [B](b.md)\r\n"
        );
    }
    #[test]
    fn edit_that_keeps_losing_the_race_writes_nothing() {
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        let path = root.join(PINNED);
        fs::write(&path, "- [A](a.md)\n").unwrap();
        let calls = std::cell::Cell::new(0);
        let result = edit_pinned(&root, |text| {
            calls.set(calls.get() + 1);
            fs::write(&path, format!("external {}", calls.get())).unwrap();
            pinned::pin(text, Kind::Note, "b.md", "B")
        });
        assert_eq!(result, Err(Error::PinnedChanged));
        assert_eq!(fs::read_to_string(&path).unwrap(), "external 3");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    }
    #[test]
    fn pinned_state_flags_missing_notes_and_collapsed_items() {
        let t = Temp::new();
        let (root, store) = pinned_setup(&t);
        fs::write(root.join("a.md"), "").unwrap();
        fs::write(
            root.join(PINNED),
            "- [A](a.md)\n- [Gone](gone.md)\n- [Page](https://example.com/)\n",
        )
        .unwrap();
        let collapsed = BTreeSet::from(["a.md".to_owned()]);
        let state = pinned_state(&root, &store, &collapsed).unwrap();
        let flags: Vec<_> = state
            .items
            .iter()
            .map(|i| (&*i.item.target, i.missing, i.collapsed))
            .collect();
        assert_eq!(
            flags,
            [
                ("a.md", false, true),
                ("gone.md", true, false),
                ("https://example.com/", false, false)
            ]
        );
        assert!(!state.restorable);
    }
    #[test]
    fn deleted_pinned_file_is_restorable_and_comes_back_unchanged() {
        let t = Temp::new();
        let (root, store) = pinned_setup(&t);
        let text = "# Mine\r\n- [A](a.md)\r\n";
        fs::write(root.join(PINNED), text).unwrap();
        pinned_state(&root, &store, &no_collapsed()).unwrap();
        fs::remove_file(root.join(PINNED)).unwrap();
        let state = pinned_state(&root, &store, &no_collapsed()).unwrap();
        assert!(state.items.is_empty() && state.restorable);
        restore_pinned_in(&root, &store).unwrap();
        assert_eq!(fs::read_to_string(root.join(PINNED)).unwrap(), text);
    }
    #[test]
    fn restore_never_overwrites_an_existing_file() {
        let t = Temp::new();
        let (root, store) = pinned_setup(&t);
        fs::write(root.join(PINNED), "- [Old](old.md)\n").unwrap();
        pinned_state(&root, &store, &no_collapsed()).unwrap();
        fs::write(root.join(PINNED), "- [New](new.md)\n").unwrap();
        restore_pinned_in(&root, &store).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(PINNED)).unwrap(),
            "- [New](new.md)\n"
        );
    }
    #[test]
    fn pinning_after_a_deletion_replaces_the_restorable_copy() {
        let t = Temp::new();
        let (root, store) = pinned_setup(&t);
        fs::write(root.join(PINNED), "- [Old](old.md)\n").unwrap();
        pinned_state(&root, &store, &no_collapsed()).unwrap();
        fs::remove_file(root.join(PINNED)).unwrap();
        edit_pinned(&root, |text| pinned::pin(text, Kind::Note, "new.md", "New")).unwrap();
        let state = pinned_state(&root, &store, &no_collapsed()).unwrap();
        assert!(!state.restorable);
        assert_eq!(state.items.len(), 1);
    }
    #[test]
    fn a_copy_is_not_offered_for_another_folder() {
        let t = Temp::new();
        let (root, store) = pinned_setup(&t);
        let other = t.0.join("other");
        fs::create_dir(&other).unwrap();
        fs::write(root.join(PINNED), "- [A](a.md)\n").unwrap();
        pinned_state(&root, &store, &no_collapsed()).unwrap();
        assert!(
            !pinned_state(&other, &store, &no_collapsed())
                .unwrap()
                .restorable
        );
        restore_pinned_in(&other, &store).unwrap();
        assert!(!other.join(PINNED).exists());
    }
    #[test]
    fn replaces_configuration() {
        let t = Temp::new();
        let p = t.0.join("settings.json");
        // Missing or corrupt settings read as the defaults.
        assert!(read_settings(&p).is_none());
        fs::write(&p, "{").unwrap();
        assert!(read_settings(&p).is_none());
        let settings = |name| Settings {
            folder: Some(t.0.join(name)),
            side: Side::Left,
            ..Settings::default()
        };
        save(&p, &settings("a")).unwrap();
        save(&p, &settings("b")).unwrap();
        assert_eq!(read_settings(&p), Some(t.0.join("b")));
        assert_eq!(load(&p).side, Side::Left);
        // Older files have no side; an unknown side reads as right and keeps the folder.
        fs::write(&p, r#"{"folder":"x","side":"top"}"#).unwrap();
        assert_eq!(load(&p).side, Side::Right);
        assert_eq!(read_settings(&p), Some(PathBuf::from("x")));
    }
    #[test]
    fn restores_leftovers_without_overwriting() {
        let t = Temp::new();
        fs::create_dir_all(t.0.join("sub")).unwrap();
        fs::create_dir(t.0.join(".oculta")).unwrap();
        for (p, text) in [
            ("a.md", "atual"),
            ("a.md.7-1.docdock-bak", "antiga"),
            ("sub/b.md.7-2.docdock-bak", "b"),
            ("a.md.7-3.docdock-tmp", "x"),
            ("a.md.docdock-bak", "x"),
            ("x.txt.7-4.docdock-bak", "x"),
            (".oculta/c.md.7-5.docdock-bak", "x"),
        ] {
            fs::write(t.0.join(p), text).unwrap();
        }
        let found = leftovers_in(&t.0).unwrap();
        let pairs: Vec<_> = found.iter().map(|l| (&*l.path, &*l.note)).collect();
        assert_eq!(
            pairs,
            [
                ("a.md.7-1.docdock-bak", "a.md"),
                ("sub/b.md.7-2.docdock-bak", "sub/b.md")
            ]
        );
        // Only leftovers can be restored, and only from inside the folder.
        assert_eq!(restore_in(&t.0, "a.md", "r"), Err(Error::InvalidPath));
        assert!(restore_in(&t.0, "../a.md.7-1.docdock-bak", "r").is_err());

        assert_eq!(
            restore_in(&t.0, "sub/b.md.7-2.docdock-bak", "r"),
            Ok("sub/b.md".into())
        );
        assert_eq!(fs::read_to_string(t.0.join("sub/b.md")).unwrap(), "b");
        // The note exists: the leftover takes a new name and the note stays as it was.
        assert_eq!(
            restore_in(&t.0, "a.md.7-1.docdock-bak", "r"),
            Ok("a (r).md".into())
        );
        fs::write(t.0.join("a.md.7-6.docdock-bak"), "outra").unwrap();
        assert_eq!(
            restore_in(&t.0, "a.md.7-6.docdock-bak", "r"),
            Ok("a (r 2).md".into())
        );
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "atual");
        assert_eq!(fs::read_to_string(t.0.join("a (r).md")).unwrap(), "antiga");
        assert_eq!(fs::read_to_string(t.0.join("a (r 2).md")).unwrap(), "outra");
        assert!(leftovers_in(&t.0).unwrap().is_empty());
    }
}
