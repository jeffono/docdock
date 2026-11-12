//! Local note storage and validation.
use crate::dock::{Request, Side};
use crate::error::Error;
use crate::i18n::Locale;
use crate::pinned::{self, Kind};
use crate::platform::{self, hold_ancestry};
use crate::trash;
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
    #[serde(default, deserialize_with = "theme_or_default")]
    theme: Theme,
    #[serde(default)]
    search_engine: crate::web::SearchEngine,
    #[serde(default, deserialize_with = "order_or_default")]
    note_order: NoteOrder,
    #[serde(default, deserialize_with = "bool_or_default")]
    group_by_date: bool,
    #[serde(default, deserialize_with = "bool_or_default")]
    relative_dates: bool,
    /// Wins over `group_by_date`; two flags keep settings written before folder grouping valid.
    #[serde(default, deserialize_with = "bool_or_default")]
    group_by_folder: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<PathBuf>,
    #[serde(default, deserialize_with = "side_or_default")]
    side: Side,
    /// Panel width in logical pixels; absent for the default.
    #[serde(
        default,
        deserialize_with = "width_or_default",
        skip_serializing_if = "Option::is_none"
    )]
    width: Option<u32>,
    /// Activation shortcut as shown, such as `Ctrl+Alt+Space`; absent for the default.
    #[serde(
        default,
        deserialize_with = "text_or_default",
        skip_serializing_if = "Option::is_none"
    )]
    activation_shortcut: Option<String>,
    /// Pinned targets shown as one line on this computer; the rest start as previews.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    collapsed: BTreeSet<String>,
}
#[derive(Default, Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}
impl Theme {
    pub fn native(self) -> Option<tauri::Theme> {
        match self {
            Self::System => None,
            Self::Light => Some(tauri::Theme::Light),
            Self::Dark => Some(tauri::Theme::Dark),
        }
    }
}
fn theme_or_default<'de, D: Deserializer<'de>>(value: D) -> Result<Theme, D::Error> {
    Ok(Theme::deserialize(serde_json::Value::deserialize(value)?).unwrap_or_default())
}
pub fn theme(app: &AppHandle) -> Theme {
    config(app).map(|p| load(&p).theme).unwrap_or_default()
}
pub fn set_theme(app: &AppHandle, theme: Theme) -> Result<(), Error> {
    update(app, |settings| settings.theme = theme)?;
    // Saved already: the page applies it either way, so a window that refuses only keeps its frame.
    for window in app.webview_windows().values() {
        if let Err(error) = window.set_theme(theme.native()) {
            eprintln!("[theme] failed to set the window theme: {error:?}");
        }
    }
    Ok(())
}
#[derive(Default, Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteOrder {
    NameAsc,
    NameDesc,
    ModifiedAsc,
    #[default]
    ModifiedDesc,
    CreatedAsc,
    CreatedDesc,
}
fn order_or_default<'de, D: Deserializer<'de>>(value: D) -> Result<NoteOrder, D::Error> {
    Ok(NoteOrder::deserialize(serde_json::Value::deserialize(value)?).unwrap_or_default())
}
pub fn note_order(app: &AppHandle) -> Result<NoteOrder, Error> {
    Ok(load(&config(app)?).note_order)
}
pub fn set_note_order(app: &AppHandle, order: NoteOrder) -> Result<(), Error> {
    update(app, |settings| settings.note_order = order)
}
pub fn relative_dates(app: &AppHandle) -> Result<bool, Error> {
    Ok(load(&config(app)?).relative_dates)
}
pub fn set_relative_dates(app: &AppHandle, enabled: bool) -> Result<(), Error> {
    update(app, |settings| settings.relative_dates = enabled)
}
fn bool_or_default<'de, D: Deserializer<'de>>(value: D) -> Result<bool, D::Error> {
    Ok(bool::deserialize(serde_json::Value::deserialize(value)?).unwrap_or_default())
}
#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grouping {
    None,
    Date,
    Folder,
}
impl Settings {
    fn grouping(&self) -> Grouping {
        if self.group_by_folder {
            Grouping::Folder
        } else if self.group_by_date {
            Grouping::Date
        } else {
            Grouping::None
        }
    }
}
pub fn grouping(app: &AppHandle) -> Result<Grouping, Error> {
    Ok(load(&config(app)?).grouping())
}
pub fn set_grouping(app: &AppHandle, grouping: Grouping) -> Result<(), Error> {
    update(app, |settings| {
        settings.group_by_date = grouping == Grouping::Date;
        settings.group_by_folder = grouping == Grouping::Folder;
    })
}
/// An unknown side must not make the folder unreadable.
fn side_or_default<'de, D: Deserializer<'de>>(value: D) -> Result<Side, D::Error> {
    Ok(Side::deserialize(serde_json::Value::deserialize(value)?).unwrap_or_default())
}
/// An invalid width must not make the folder unreadable either.
fn width_or_default<'de, D: Deserializer<'de>>(value: D) -> Result<Option<u32>, D::Error> {
    Ok(Option::<u32>::deserialize(serde_json::Value::deserialize(value)?).unwrap_or_default())
}
fn text_or_default<'de, D: Deserializer<'de>>(value: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(serde_json::Value::deserialize(value)?).unwrap_or_default())
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
    /// Last write, in milliseconds since the Unix epoch; 0 if unknown.
    modified: u64,
    created: u64,
}
/// One note as the search index needs it.
#[derive(Serialize, Debug, PartialEq)]
pub struct NoteText {
    path: String,
    text: String,
    modified: u64,
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
pub fn search_engine(app: &AppHandle) -> crate::web::SearchEngine {
    config(app)
        .map(|p| load(&p).search_engine)
        .unwrap_or_default()
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
pub fn width(app: &AppHandle) -> Option<u32> {
    config(app).ok().and_then(|p| load(&p).width)
}
pub fn set_width(app: &AppHandle, width: Option<u32>) -> Result<(), Error> {
    update(app, |settings| settings.width = width)
}
pub fn activation_shortcut(app: &AppHandle) -> Option<String> {
    config(app).ok().and_then(|p| load(&p).activation_shortcut)
}
pub fn set_activation_shortcut(app: &AppHandle, shortcut: Option<String>) -> Result<(), Error> {
    update(app, |settings| settings.activation_shortcut = shortcut)
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
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
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
        purge_trash(app);
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
                created: fs::metadata(&path)
                    .and_then(|m| m.created())
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or_else(|| modified(&path)),
                modified: modified(&path),
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
/// A path the UI sent, before it touches the disk: relative, with `/`, only plain names and `allowed`.
fn checked(relative: &str, allowed: fn(&Path) -> bool) -> Result<&Path, Error> {
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
    Ok(path)
}
/// `allowed` applies to both the request and canonical target: an 8.3 short name or symlink inside
/// the folder can change the requested extension to a different real extension.
fn resolve(root: &Path, relative: &str, allowed: fn(&Path) -> bool) -> Result<PathBuf, Error> {
    let path = checked(relative, allowed)?;
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
/// Reads many notes at once for the search index, with the same path rules as `open`. A note that
/// cannot be read (gone, too large, not UTF-8, outside the folder) is left out; the rest are returned.
fn read_many(root: &Path, paths: &[String]) -> Vec<NoteText> {
    paths
        .iter()
        .filter_map(|relative| {
            let path = resolve(root, relative, markdown).ok()?;
            let text = read_note(&path).ok()?;
            Some(NoteText {
                path: relative.clone(),
                text,
                modified: modified(&path),
            })
        })
        .collect()
}
pub fn texts(app: &AppHandle, paths: &[String]) -> Result<Vec<NoteText>, Error> {
    Ok(read_many(&root(app)?, paths))
}
/// Serializes saves so the conflict check and the write are not interleaved within the app.
static SAVING: Mutex<()> = Mutex::new(());
/// Writes `content` only if the file still holds the version identified by `base`. The early check
/// avoids a write in the common cases; `replace_note` confirms it against the version actually replaced.
fn save_note_in(root: &Path, relative: &str, content: &str, base: &str) -> Result<Saved, Error> {
    write_note_in(root, relative, content, base, true)
}
/// As `save_note_in`, but a missing note answers `Conflict` instead of being recreated when
/// `recreate` is false: for a change the app makes on its own, which must not bring back a note
/// deleted or renamed elsewhere.
fn write_note_in(
    root: &Path,
    relative: &str,
    content: &str,
    base: &str,
    recreate: bool,
) -> Result<Saved, Error> {
    too_large(content.len() as u64)?;
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    if reserved(root, relative) {
        return Ok(Saved::Conflict);
    }
    let target = match resolve(root, relative, markdown) {
        Ok(target) if !base.is_empty() => target,
        Ok(_) => return Ok(Saved::Conflict),
        Err(_) if !exists(&root.join(relative)) => {
            if !recreate {
                return Ok(Saved::Conflict);
            }
            return recreate_note(root, relative, content.as_bytes());
        }
        Err(error) => return Err(error),
    };
    let current = match read_bytes(&target) {
        Ok(current) => current,
        Err(_) if !exists(&target) && !recreate => return Ok(Saved::Conflict),
        Err(_) if !exists(&target) => return recreate_note(root, relative, content.as_bytes()),
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
fn recreate_note(root: &Path, relative: &str, content: &[u8]) -> Result<Saved, Error> {
    let path = checked(relative, markdown)?;
    let root = root.canonicalize()?;
    let target = root.join(path);
    let parent = target.parent().ok_or(Error::InvalidPath)?;
    let _held = hold_ancestry(parent)?;
    if !parent.canonicalize()?.starts_with(&root) {
        return Err(Error::OutsideFolder);
    }
    if exists(&target) {
        return Ok(Saved::Conflict);
    }
    let temp = write_temp(&target, content)?;
    let result = match platform::move_new(&temp, &target) {
        Ok(()) => Ok(Saved::Saved {
            version: version(content),
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
pub fn rewrite_note(
    app: &AppHandle,
    path: &str,
    content: &str,
    base: &str,
) -> Result<Saved, Error> {
    write_note_in(&root(app)?, path, content, base, false)
}
/// Renames the note `from` to `to`, in the same folder. See `relocate`.
fn rename_note_in(
    root: &Path,
    from: &str,
    to: &str,
    base: &str,
    title: (&str, &str),
) -> Result<(), Error> {
    if Path::new(from).parent() != Path::new(to).parent() {
        return Err(Error::InvalidPath);
    }
    relocate(root, from, to, base, title)
}
/// Moves the note `from` to `to`, in another folder that exists and is not hidden, keeping the link
/// text of its pinned items. See `relocate`.
fn move_note_in(root: &Path, from: &str, to: &str, base: &str) -> Result<(), Error> {
    let folder = Path::new(to).parent().unwrap_or(Path::new(""));
    let hidden = folder
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
    let current = Path::new(from).parent().unwrap_or(Path::new(""));
    if hidden || same_name(folder, current) {
        return Err(Error::InvalidPath);
    }
    // An empty new title never replaces a link text.
    relocate(root, from, to, base, ("", ""))
}
/// Moves the note `from` to `to`, never over another file: a taken name answers `RenameConflict`, as
/// do a note that no longer holds `base` and a path a deleted note reserves. A case-only change renames
/// the same file. Pinned items of `from` follow it; `title` is the old and new title for their link
/// text. If they cannot, the answer is `PinnedNotRenamed`: the note is moved all the same.
fn relocate(
    root: &Path,
    from: &str,
    to: &str,
    base: &str,
    title: (&str, &str),
) -> Result<(), Error> {
    let (source, target) = (checked(from, markdown)?, checked(to, markdown)?);
    let pinned = |path: &Path| path.as_os_str().eq_ignore_ascii_case(PINNED);
    if pinned(source) || pinned(target) {
        return Err(Error::InvalidPath);
    }
    {
        let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        if reserved(root, to) {
            return Err(Error::RenameConflict);
        }
        let source = resolve(root, from, markdown)?;
        let _held = hold_ancestry(source.parent().ok_or(Error::InvalidPath)?)?;
        if version(&read_bytes(&source)?) != base {
            return Err(Error::RenameConflict);
        }
        let (folder, _held_target) = hold_folder(root, target.parent().ok_or(Error::InvalidPath)?)?;
        let target = folder.join(target.file_name().ok_or(Error::InvalidPath)?);
        // Without replacing, `MoveFileExW` refuses a taken name and allows a case-only change.
        if target != source
            && let Err(error) = platform::move_new(&source, &target)
        {
            return Err(if exists(&target) && !same_name(&source, &target) {
                Error::RenameConflict
            } else {
                error.into()
            });
        }
    }
    // The move happened: a failure here must say so, since `from` no longer exists to retry.
    edit_pinned(root, |text| pinned::rename(text, from, to, title)).map_err(|error| {
        eprintln!("[move] pinned items not moved: {error:?}");
        Error::PinnedNotRenamed {
            path: to.to_owned(),
        }
    })
}
/// An existing folder below `root`, or `root` itself for an empty path, held with its ancestry while
/// the handles live. The path as named is held, not its canonical form: a link at any level, even one
/// pointing inside `root`, is refused, so the folder written to is the one named, and the reservations
/// and pinned items checked against that name apply to it.
fn hold_folder(root: &Path, relative: &Path) -> Result<(PathBuf, Vec<fs::File>), Error> {
    let folder = root.canonicalize()?.join(relative);
    let held = hold_ancestry(&folder).map_err(|_| Error::OutsideFolder)?;
    Ok((folder, held))
}
/// Windows compares names ignoring case.
fn same_name(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}
pub fn rename_note(
    app: &AppHandle,
    from: &str,
    to: &str,
    base: &str,
    title: (&str, &str),
) -> Result<(), Error> {
    let renamed = rename_note_in(&root(app)?, from, to, base, title);
    follow(app, from, to, &renamed);
    renamed
}
pub fn move_note(app: &AppHandle, from: &str, to: &str, base: &str) -> Result<(), Error> {
    let moved = move_note_in(&root(app)?, from, to, base);
    follow(app, from, to, &moved);
    moved
}
/// Settings that name a moved note follow it.
fn follow(app: &AppHandle, from: &str, to: &str, result: &Result<(), Error>) {
    if matches!(result, Ok(()) | Err(Error::PinnedNotRenamed { .. })) {
        // Best effort: the note has moved, and an error here would hide that from the editor, whose
        // next save would bring the old name back. A lost collapsed state only shows the card open.
        let _ = update(app, |settings| {
            let before = settings.collapsed.len();
            settings
                .collapsed
                .retain(|target| target.to_lowercase() != from.to_lowercase());
            if settings.collapsed.len() != before {
                settings.collapsed.insert(to.to_owned());
            }
        });
    }
}
/// Every folder below `root` that listing visits, as `/` paths sorted as notes are: hidden folders and
/// links are left out.
fn folders_in(root: &Path) -> Result<Vec<String>, Error> {
    fn visit(root: &Path, dir: &Path, found: &mut Vec<String>) -> Result<(), Error> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && !entry.file_name().to_string_lossy().starts_with('.')
            {
                found.push(relative(root, &entry.path()));
                visit(root, &entry.path(), found)?;
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    visit(root, root, &mut found)?;
    found.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b)));
    Ok(found)
}
#[derive(serde::Serialize)]
pub struct FolderEntry {
    path: String,
    empty: bool,
}
pub fn folders(app: &AppHandle) -> Result<Vec<FolderEntry>, Error> {
    let root = root(app)?;
    folders_in(&root)?
        .into_iter()
        .map(|path| {
            let empty = fs::read_dir(root.join(&path))?
                .next()
                .transpose()?
                .is_none();
            Ok(FolderEntry { path, empty })
        })
        .collect()
}
/// A folder name Windows keeps as typed and listing shows: not hidden, no leading or trailing space,
/// no trailing dot (Windows drops it), no character a name cannot hold and no device name, even with
/// an extension.
fn folder_name_allowed(name: &str) -> bool {
    let device = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_lowercase();
    let numbered = (device.starts_with("com") || device.starts_with("lpt"))
        && device.chars().count() == 4
        && device
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_digit() || "¹²³".contains(c));
    !name.is_empty()
        && name.chars().count() <= 100
        && !name.starts_with(['.', ' '])
        && !name.ends_with(['.', ' '])
        && !name.chars().any(|c| c < ' ' || r#"<>:"/\|?*"#.contains(c))
        && !["con", "prn", "aux", "nul"].contains(&device.as_str())
        && !numbered
}
/// Creates the folder `relative` below `root`, with `/` between nested names, and any missing folder
/// above it; one that exists is kept. Each level must be a real folder inside `root`, never a link.
fn create_folder_in(root: &Path, relative: &str) -> Result<(), Error> {
    let names: Vec<&str> = relative.split('/').collect();
    // A folder in the place of the pinned list would hide it.
    if !names.iter().all(|name| folder_name_allowed(name)) || names[0].eq_ignore_ascii_case(PINNED)
    {
        return Err(Error::InvalidFolderName);
    }
    let mut dir = root.canonicalize()?;
    for name in names {
        // Holding rejects a link at any level, such as one that took a name meanwhile.
        let _held = hold_ancestry(&dir).map_err(|_| Error::OutsideFolder)?;
        dir.push(name);
        match fs::create_dir(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if fs::symlink_metadata(&dir).is_ok_and(|m| m.is_file()) {
                    return Err(Error::FolderNameTaken {
                        name: name.to_owned(),
                    });
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    hold_ancestry(&dir).map_err(|_| Error::OutsideFolder)?;
    Ok(())
}
pub fn create_folder(app: &AppHandle, path: &str) -> Result<(), Error> {
    create_folder_in(&root(app)?, path)
}
/// True only for descendants, with a segment boundary and Windows name comparison.
fn inside_folder(path: &str, folder: &str) -> bool {
    path.to_lowercase()
        .starts_with(&(folder.to_lowercase() + "/"))
}
#[derive(Debug, PartialEq, serde::Serialize, Default)]
pub struct FolderCounts {
    notes: usize,
    other: usize,
}
/// Count every file, including hidden content; never follow links out of the folder.
fn folder_counts(path: &Path) -> Result<FolderCounts, Error> {
    let _held = hold_ancestry(path)?;
    let mut counts = FolderCounts::default();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            let nested = folder_counts(&entry.path())?;
            counts.notes += nested.notes;
            counts.other += nested.other;
        } else if kind.is_file() && markdown(&entry.path()) {
            counts.notes += 1;
        } else {
            counts.other += 1;
        }
    }
    Ok(counts)
}
/// An entry arriving after the empty check must survive and trigger confirmation.
fn remove_empty_folder(target: &Path) -> Result<Option<FolderCounts>, Error> {
    match fs::remove_dir(target) {
        Ok(()) => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => {
            Ok(Some(folder_counts(target)?))
        }
        Err(error) => Err(error.into()),
    }
}
/// None means deleted; Some(count) asks the caller to confirm recycling content. Empty removal is
/// non-recursive, so an entry arriving after the check can never be permanently deleted.
fn delete_folder_in(
    root: &Path,
    relative: &str,
    confirmed: bool,
    recycle: impl Fn(&Path) -> io::Result<Option<PathBuf>>,
) -> Result<Option<FolderCounts>, Error> {
    if !relative.split('/').all(|name| {
        !name.is_empty()
            && !name.starts_with('.')
            && !name.ends_with(['.', ' '])
            && !name.eq_ignore_ascii_case(PINNED)
    }) {
        return Err(Error::InvalidPath);
    }
    let path = checked(relative, |_| true)?;
    let session = deleted().session;
    {
        let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        let (target, mut held) = hold_folder(root, path)?;
        // Refuse short-name aliases too: pins must name the folder being deleted.
        if !same_name(&target, &target.canonicalize()?) {
            return Err(Error::InvalidPath);
        }
        let empty = fs::read_dir(&target)?.next().transpose()?.is_none();
        if !empty && !confirmed {
            return Ok(Some(folder_counts(&target)?));
        }
        // Keep all parents held; the target itself must allow the shell/remove_dir to move it.
        held.pop();
        if empty {
            if let Some(counts) = remove_empty_folder(&target)? {
                return Ok(Some(counts));
            }
        } else {
            let landed = recycle(&target).map_err(|error| match error.kind() {
                io::ErrorKind::Unsupported => Error::NotRecyclable,
                _ => Error::DeleteFailed {
                    detail: error.to_string(),
                },
            })?;
            let mut trash = deleted();
            // Only the notes folder's own trash gives a folder back; the Recycle Bin restores it.
            if let Some(trashed) = landed.filter(|landed| trash::holds(root, landed))
                && trash.session == session
            {
                trash.folders.push(DeletedFolder {
                    root: root.to_owned(),
                    path: relative.to_owned(),
                    trashed,
                    pinned: None,
                });
            }
        }
        let mut trash = deleted();
        trash
            .notes
            .retain(|d| d.root != root || !inside_folder(&d.path, relative));
        trash
            .folders
            .retain(|d| d.root != root || !inside_folder(&d.path, relative));
    }
    let undo = folder_undo(root, relative).is_some();
    let change = std::cell::RefCell::new(None);
    edit_pinned(root, |text| {
        let edited = pinned::parse(text)
            .iter()
            .filter(|item| item.kind == Kind::Note && inside_folder(&item.target, relative))
            .fold(text.to_owned(), |text, item| {
                pinned::unpin(&text, &item.target)
            });
        *change.borrow_mut() = (edited != text).then(|| (text.to_owned(), edited.clone()));
        edited
    })
    .map_err(|error| {
        eprintln!("[delete folder] pinned items not removed: {error:?}");
        Error::PinnedNotRemoved { undo }
    })?;
    if let Some(at) = folder_undo(root, relative) {
        deleted().folders[at].pinned = change.into_inner();
    }
    Ok(None)
}
/// Where Undo keeps the folder `relative` deleted from `root`, if it does.
fn folder_undo(root: &Path, relative: &str) -> Option<usize> {
    deleted()
        .folders
        .iter()
        .position(|d| d.root == root && d.path == relative)
}
/// Moves a folder deleted to the notes folder's own trash back to its path, never over a file or
/// folder that took the name since (`RestoreTaken`, and Undo stays possible). Its pinned items return
/// as a note's do.
fn undo_delete_folder_in(root: &Path, relative: &str) -> Result<(), Error> {
    let pinned = {
        let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        let mut trash = deleted();
        let at = trash
            .folders
            .iter()
            .position(|d| d.root == root && d.path == relative)
            .ok_or(Error::UndoUnavailable)?;
        let path = checked(relative, |_| true)?;
        let (parent, _held) = hold_folder(root, path.parent().ok_or(Error::InvalidPath)?)?;
        let target = parent.join(path.file_name().ok_or(Error::InvalidPath)?);
        let trashed = &trash.folders[at].trashed;
        let _held_trash = hold_ancestry(trashed.parent().ok_or(Error::InvalidPath)?)?;
        if let Err(error) = platform::move_new(trashed, &target) {
            return Err(if exists(&target) {
                Error::RestoreTaken {
                    path: relative.to_owned(),
                }
            } else {
                error.into()
            });
        }
        trash.folders.remove(at).pinned
    };
    if let Some((before, after)) = pinned
        && exists(&root.join(PINNED))
    {
        // Pinning a target again adds nothing: each comes back once.
        let targets: Vec<String> = pinned::parse(&before)
            .into_iter()
            .filter(|item| item.kind == Kind::Note && inside_folder(&item.target, relative))
            .map(|item| item.target)
            .collect();
        edit_pinned(root, |text| {
            if text == after {
                return before.clone();
            }
            targets.iter().fold(text.to_owned(), |text, target| {
                pinned::repin(&text, &before, &after, target)
            })
        })
        .map_err(|error| {
            eprintln!("[undo folder] pinned items not restored: {error:?}");
            Error::PinnedNotRestored
        })?;
    }
    Ok(())
}
/// `binless`: the user agreed to delete for good what the Recycle Bin would not take, once the panel
/// closes; until then it waits in the notes folder's own trash, where Undo finds it.
pub fn delete_folder(
    app: &AppHandle,
    path: &str,
    confirmed: bool,
    binless: bool,
) -> Result<Option<FolderCounts>, Error> {
    let root = root(app)?;
    if binless {
        delete_folder_in(&root, path, confirmed, |target| to_trash(&root, target))
    } else {
        delete_folder_in(&root, path, confirmed, platform::recycle)
    }
}
pub fn undo_delete_folder(app: &AppHandle, path: &str) -> Result<(), Error> {
    undo_delete_folder_in(&root(app)?, path)
}
/// Moves `target` to the notes folder's own trash, named after the current panel session; called
/// under the save lock, as the Recycle Bin is.
fn to_trash(root: &Path, target: &Path) -> io::Result<Option<PathBuf>> {
    let session = {
        let mut trash = deleted();
        if !trash.roots.iter().any(|r| r == root) {
            trash.roots.push(root.to_owned());
        }
        trash.session
    };
    trash::put(root, target, session).map(Some)
}
/// A note moved to the Recycle Bin from the app, kept in memory until the panel closes so Undo can
/// write it back without depending on the Recycle Bin's layout. Its path stays reserved meanwhile:
/// saves and renames to it answer a conflict, so an editor still open on it cannot recreate it and no
/// new note takes the name Undo needs.
struct Deleted {
    root: PathBuf,
    path: String,
    bytes: Vec<u8>,
    /// `_pinned.md` before and after the note's items were removed.
    pinned: Option<(String, String)>,
}
/// The notes deleted since the panel last hid. `session` counts the hides, so a delete that finishes
/// after one keeps nothing for Undo. Locked after `SAVING` when both are needed.
struct Trash {
    session: u64,
    notes: Vec<Deleted>,
    folders: Vec<DeletedFolder>,
    /// Every notes folder whose own trash took something in this run, purged when the panel hides.
    roots: Vec<PathBuf>,
}
/// A folder moved to the notes folder's own trash (`trash.rs`); Undo moves it back from there.
struct DeletedFolder {
    root: PathBuf,
    path: String,
    trashed: PathBuf,
    /// `_pinned.md` before and after the folder's items were removed.
    pinned: Option<(String, String)>,
}
static DELETED: Mutex<Trash> = Mutex::new(Trash {
    session: 0,
    notes: Vec::new(),
    folders: Vec::new(),
    roots: Vec::new(),
});
fn deleted() -> std::sync::MutexGuard<'static, Trash> {
    DELETED.lock().unwrap_or_else(|e| e.into_inner())
}
fn reserved(root: &Path, relative: &str) -> bool {
    let trash = deleted();
    trash
        .notes
        .iter()
        .any(|d| d.root == root && same_name(Path::new(&d.path), Path::new(relative)))
}
/// Moves the note to the Recycle Bin through `recycle`, which answers where the file landed there.
/// Undo keeps the bytes read from that file, so they are what the Recycle Bin holds even if another
/// app swapped the note's path meanwhile. A note the Recycle Bin cannot take stays. Answers whether
/// Undo is possible: not when the panel hid during the delete, or the Recycle Bin copy cannot be read.
/// Its pinned items are removed after; if they cannot be, the answer is `PinnedNotRemoved`: the note
/// is in the Recycle Bin all the same.
fn delete_note_in(
    root: &Path,
    relative: &str,
    recycle: impl Fn(&Path) -> io::Result<Option<PathBuf>>,
) -> Result<bool, Error> {
    if checked(relative, markdown)?
        .as_os_str()
        .eq_ignore_ascii_case(PINNED)
    {
        return Err(Error::InvalidPath);
    }
    let session = deleted().session;
    let undo = {
        let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        let target = resolve(root, relative, markdown)?;
        let _held = hold_ancestry(target.parent().ok_or(Error::InvalidPath)?)?;
        // A note too large to keep for Undo is not deleted from the app.
        too_large(fs::metadata(&target)?.len())?;
        let recycled = recycle(&target).map_err(|error| match error.kind() {
            io::ErrorKind::Unsupported => Error::NotRecyclable,
            _ => Error::DeleteFailed {
                detail: error.to_string(),
            },
        })?;
        let bytes = recycled.and_then(|bin| read_bytes(&bin).ok());
        let mut trash = deleted();
        match bytes {
            Some(bytes) if trash.session == session => {
                trash.notes.push(Deleted {
                    root: root.to_owned(),
                    path: relative.to_owned(),
                    bytes,
                    pinned: None,
                });
                true
            }
            _ => false,
        }
    };
    let change = std::cell::RefCell::new(None);
    edit_pinned(root, |text| {
        let edited = pinned::unpin(text, relative);
        *change.borrow_mut() = (edited != text).then(|| (text.to_owned(), edited.clone()));
        edited
    })
    .map_err(|error| {
        eprintln!("[delete] pinned items not removed: {error:?}");
        Error::PinnedNotRemoved { undo }
    })?;
    if let Some(entry) = deleted()
        .notes
        .iter_mut()
        .find(|d| d.root == root && d.path == relative)
    {
        entry.pinned = change.into_inner();
    }
    Ok(undo)
}
/// Writes a deleted note back at its path, never over a file that took the name since
/// (`RestoreTaken`, and Undo stays possible). Its pinned items return too, in place if `_pinned.md`
/// did not change since.
fn undo_delete_in(root: &Path, relative: &str) -> Result<(), Error> {
    let pinned = {
        let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        let mut trash = deleted();
        let deleted = &mut trash.notes;
        let at = deleted
            .iter()
            .position(|d| d.root == root && d.path == relative)
            .ok_or(Error::UndoUnavailable)?;
        if let Saved::Conflict = recreate_note(root, relative, &deleted[at].bytes)? {
            return Err(Error::RestoreTaken {
                path: relative.to_owned(),
            });
        }
        deleted.remove(at).pinned
    };
    // A deleted list is left for its own restore, rather than created with only these items.
    if let Some((before, after)) = pinned
        && exists(&root.join(PINNED))
    {
        edit_pinned(root, |text| pinned::repin(text, &before, &after, relative)).map_err(
            |error| {
                eprintln!("[undo] pinned items not restored: {error:?}");
                Error::PinnedNotRestored
            },
        )?;
    }
    Ok(())
}
/// `binless`: as for `delete_folder`.
pub fn delete_note(app: &AppHandle, path: &str, binless: bool) -> Result<bool, Error> {
    let root = root(app)?;
    if binless {
        delete_note_in(&root, path, |target| to_trash(&root, target))
    } else {
        delete_note_in(&root, path, platform::recycle)
    }
}
pub fn undo_delete(app: &AppHandle, path: &str) -> Result<(), Error> {
    undo_delete_in(&root(app)?, path)
}
/// The panel closed: deleted notes stay in the Recycle Bin and their names are free again. What the
/// notes folders' own trash holds is deleted for good, away from the caller's thread.
pub fn forget_deleted() {
    let mut trash = deleted();
    trash.session += 1;
    trash.notes.clear();
    trash.folders.clear();
    let (roots, session) = (trash.roots.clone(), trash.session);
    std::thread::spawn(move || {
        for root in roots {
            trash::purge(&root, session);
        }
    });
}
/// Deletes for good what an earlier run left in the notes folder's own trash: at startup and when a
/// folder is chosen. The current session's entries stay.
pub fn purge_trash(app: &AppHandle) {
    let Ok(root) = root(app) else { return };
    let session = deleted().session;
    std::thread::spawn(move || trash::purge(&root, session));
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
    version: String,
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
/// Serializes reading `_pinned.md` with updating its copy, so a slower read cannot replace the copy of
/// a newer one.
static PINNED_COPY: Mutex<()> = Mutex::new(());
fn pinned_state(root: &Path, store: &Path, collapsed: &BTreeSet<String>) -> Result<Pinned, Error> {
    let guard = PINNED_COPY.lock().unwrap_or_else(|e| e.into_inner());
    let Some((text, version)) = read_pinned(root)? else {
        return Ok(Pinned {
            version: String::new(),
            items: Vec::new(),
            restorable: restorable_copy(store, root).is_some(),
        });
    };
    keep_copy(store, root, &text);
    drop(guard);
    let items = pinned::parse(&text)
        .into_iter()
        .map(|item| PinnedItem {
            missing: item.kind == Kind::Note && resolve(root, &item.target, markdown).is_err(),
            collapsed: collapsed.contains(&item.target),
            item,
        })
        .collect();
    Ok(Pinned {
        version,
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
/// Reorder only the version the UI displayed; never silently apply a stale positional intent.
fn move_pinned_in(root: &Path, target: &str, to: &str, base: &str) -> Result<(), Error> {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let (text, current) = read_pinned(root)?.ok_or(Error::PinnedChanged)?;
    if current != base {
        return Err(Error::PinnedChanged);
    }
    let edited = pinned::move_item(&text, target, to);
    if edited == text {
        return Ok(());
    }
    let path = resolve(root, PINNED, markdown)?;
    match replace_note(&path, edited.as_bytes(), base)? {
        Saved::Saved { .. } => Ok(()),
        Saved::Conflict => Err(Error::PinnedChanged),
    }
}
pub fn move_pinned(app: &AppHandle, target: &str, to: &str, base: &str) -> Result<Pinned, Error> {
    move_pinned_in(&root(app)?, target, to, base)?;
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
            // The clock alone collides between parallel tests on Windows.
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "docdock-test-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
    fn bulk_read_skips_unreadable_notes_and_returns_the_rest() {
        let t = Temp::new();
        fs::write(t.0.join("ok.md"), "fine").unwrap();
        fs::write(t.0.join("binary.md"), [0xff_u8, 0xfe]).unwrap();
        let ask = ["binary.md", "gone.md", "ok.md"].map(String::from);
        let got = read_many(&t.0, &ask);
        assert_eq!(got.len(), 1);
        assert_eq!(
            (got[0].path.as_str(), got[0].text.as_str()),
            ("ok.md", "fine")
        );
        assert!(got[0].modified > 0);
    }
    #[test]
    fn bulk_read_refuses_paths_outside_the_folder() {
        let t = Temp::new();
        let notes = t.0.join("notes");
        fs::create_dir(&notes).unwrap();
        fs::write(t.0.join("outside.md"), "secret").unwrap();
        let ask = ["../outside.md", "C:/outside.md"].map(String::from);
        assert!(read_many(&notes, &ask).is_empty());
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
        assert_eq!(
            recreate_note(&t.0, "a.md", text.as_bytes()),
            Ok(Saved::Conflict)
        );
        assert_eq!(save_note_in(&t.0, "a.md", text, ""), Ok(Saved::Conflict));
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    }
    #[test]
    fn a_rewrite_never_brings_back_a_note_deleted_since_it_was_read() {
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "a").unwrap();
        let base = version(b"a");
        fs::remove_file(t.0.join("a.md")).unwrap();
        assert_eq!(
            write_note_in(&t.0, "a.md", "rewritten", &base, false),
            Ok(Saved::Conflict)
        );
        assert!(!exists(&t.0.join("a.md")));
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
    #[test]
    fn reorder_rejects_a_stale_displayed_version() {
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        let original = "- [A](a.md)\n- [B](b.md)\n";
        fs::write(root.join(PINNED), original).unwrap();
        let base = version(original.as_bytes());
        let external = format!("# External edit\n{original}");
        fs::write(root.join(PINNED), &external).unwrap();
        assert!(matches!(
            move_pinned_in(&root, "a.md", "b.md", &base),
            Err(Error::PinnedChanged)
        ));
        assert_eq!(fs::read_to_string(root.join(PINNED)).unwrap(), external);
    }
    #[test]
    fn reorder_does_not_recreate_an_externally_deleted_file() {
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        assert!(matches!(
            move_pinned_in(&root, "a.md", "b.md", "old"),
            Err(Error::PinnedChanged)
        ));
        assert!(!root.join(PINNED).exists());
    }
    #[test]
    fn reorder_atomically_preserves_handwritten_content() {
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        let original = "# Pins\r\n- [A](a.md)\r\n<!-- keep -->\n- [B](b.md)";
        fs::write(root.join(PINNED), original).unwrap();
        move_pinned_in(&root, "b.md", "a.md", &version(original.as_bytes())).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(PINNED)).unwrap(),
            "# Pins\r\n- [B](b.md)\r\n<!-- keep -->\n- [A](a.md)"
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    }
    #[test]
    fn invalid_note_order_does_not_discard_other_settings() {
        for value in ["null", "42", "{}", "\"future-order\""] {
            let settings: Settings =
                serde_json::from_str(&format!(r#"{{"folder":"notes","note_order":{value}}}"#))
                    .unwrap();
            assert_eq!(settings.note_order, NoteOrder::ModifiedDesc);
            assert_eq!(settings.folder, Some(PathBuf::from("notes")));
        }
    }
    #[test]
    fn invalid_theme_does_not_discard_other_settings() {
        for value in ["null", "42", "{}", "[]", "true", "\"future-theme\""] {
            let settings: Settings = serde_json::from_str(&format!(
                r#"{{"folder":"notes","note_order":"created-asc","theme":{value}}}"#
            ))
            .unwrap();
            assert_eq!(settings.theme, Theme::System);
            assert_eq!(settings.note_order, NoteOrder::CreatedAsc);
            assert_eq!(settings.folder, Some(PathBuf::from("notes")));
        }
    }
    #[test]
    fn invalid_grouping_does_not_discard_other_settings() {
        for value in ["null", "42", "{}", "\"true\"", "[]"] {
            let settings: Settings = serde_json::from_str(&format!(
                r#"{{"folder":"notes","note_order":"created-asc","group_by_date":{value}}}"#
            ))
            .unwrap();
            assert_eq!(settings.grouping(), Grouping::None);
            assert_eq!(settings.note_order, NoteOrder::CreatedAsc);
            assert_eq!(settings.folder, Some(PathBuf::from("notes")));
        }
    }
    #[test]
    fn older_settings_keep_absolute_dates() {
        let settings: Settings = serde_json::from_str(r#"{"group_by_date":true}"#).unwrap();
        assert!(!settings.relative_dates);
    }
    #[test]
    fn invalid_relative_dates_does_not_discard_other_settings() {
        for value in ["null", "42", "{}", "[]", "\"true\""] {
            let settings: Settings =
                serde_json::from_str(&format!(r#"{{"folder":"notes","relative_dates":{value}}}"#))
                    .unwrap();
            assert!(!settings.relative_dates);
            assert_eq!(settings.folder, Some(PathBuf::from("notes")));
        }
    }
    #[test]
    fn invalid_activation_shortcut_does_not_discard_other_settings() {
        for value in ["null", "42", "{}", "[]"] {
            let settings: Settings = serde_json::from_str(&format!(
                r#"{{"folder":"notes","activation_shortcut":{value}}}"#
            ))
            .unwrap();
            assert_eq!(settings.activation_shortcut, None);
            assert_eq!(settings.folder, Some(PathBuf::from("notes")));
        }
    }
    #[test]
    fn folder_grouping_wins_and_older_settings_keep_date_grouping() {
        let read = |json: &str| serde_json::from_str::<Settings>(json).unwrap().grouping();
        assert_eq!(read(r#"{"group_by_date":true}"#), Grouping::Date);
        assert_eq!(
            read(r#"{"group_by_date":true,"group_by_folder":true}"#),
            Grouping::Folder
        );
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
    fn rename(root: &Path, from: &str, to: &str) -> Result<(), Error> {
        let base = version(&fs::read(root.join(from)).unwrap());
        rename_note_in(root, from, to, &base, ("A", "B"))
    }
    #[test]
    fn renaming_never_takes_an_existing_name() {
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "a").unwrap();
        fs::write(t.0.join("b.md"), "b").unwrap();
        assert_eq!(rename(&t.0, "a.md", "B.md"), Err(Error::RenameConflict));
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "a");
        assert_eq!(fs::read_to_string(t.0.join("b.md")).unwrap(), "b");
    }
    #[test]
    fn renaming_a_note_changed_since_it_was_read_does_nothing() {
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "a").unwrap();
        let base = version(b"a");
        fs::write(t.0.join("a.md"), "external").unwrap();
        let renamed = rename_note_in(&t.0, "a.md", "b.md", &base, ("A", "B"));
        assert_eq!(renamed, Err(Error::RenameConflict));
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "external");
        assert!(!exists(&t.0.join("b.md")));
    }
    #[test]
    fn renaming_moves_the_note_and_its_pinned_items() {
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/a.md"), "body").unwrap();
        fs::write(
            root.join(PINNED),
            "- [A](sub/a.md)
- [C](c.md)
",
        )
        .unwrap();
        assert_eq!(rename(&root, "sub/a.md", "sub/B.md"), Ok(()));
        assert!(!exists(&root.join("sub/a.md")));
        assert_eq!(fs::read_to_string(root.join("sub/B.md")).unwrap(), "body");
        assert_eq!(
            fs::read_to_string(root.join(PINNED)).unwrap(),
            "- [B](sub/B.md)
- [C](c.md)
"
        );
    }
    #[test]
    fn renaming_reports_the_new_name_when_the_pinned_list_cannot_follow() {
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "a").unwrap();
        // A folder under the pinned name cannot be read or written as the list.
        fs::create_dir(t.0.join(PINNED)).unwrap();
        let renamed = rename(&t.0, "a.md", "b.md");
        assert_eq!(
            renamed,
            Err(Error::PinnedNotRenamed {
                path: "b.md".into()
            })
        );
        assert_eq!(fs::read_to_string(t.0.join("b.md")).unwrap(), "a");
    }
    #[test]
    fn deleting_an_empty_folder_removes_only_that_directory() {
        let t = Temp::new();
        fs::create_dir_all(t.0.join("parent/empty")).unwrap();
        assert_eq!(
            delete_folder_in(&t.0, "parent/empty", false, |_| panic!(
                "empty folder must not recycle"
            )),
            Ok(None)
        );
        assert!(!t.0.join("parent/empty").exists());
        assert!(t.0.join("parent").is_dir());
    }
    #[test]
    fn deleting_folder_content_requires_confirmation_even_for_hidden_files() {
        let t = Temp::new();
        fs::create_dir(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/.hidden"), "keep").unwrap();
        assert_eq!(
            delete_folder_in(&t.0, "folder", false, |_| panic!("must confirm")),
            Ok(Some(FolderCounts { notes: 0, other: 1 }))
        );
        assert_eq!(
            fs::read_to_string(t.0.join("folder/.hidden")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn a_folder_the_recycle_bin_refuses_is_never_removed_permanently() {
        let t = Temp::new();
        fs::create_dir(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/a.md"), "keep").unwrap();
        assert_eq!(
            delete_folder_in(&t.0, "folder", true, |_| Err(
                io::ErrorKind::Unsupported.into()
            )),
            Err(Error::NotRecyclable)
        );
        assert_eq!(fs::read_to_string(t.0.join("folder/a.md")).unwrap(), "keep");
    }
    #[test]
    fn deleting_a_folder_removes_only_pins_inside_it() {
        let t = Temp::new();
        fs::create_dir_all(t.0.join("folder/sub")).unwrap();
        fs::write(t.0.join("folder/sub/a.md"), "keep in bin").unwrap();
        fs::write(t.0.join(PINNED), "# Pins\n- [A](FOLDER/sub/a.md)\n- [B](folder-other/a.md)\n- [Web](https://example.com/folder/a.md)\n").unwrap();
        let bin = t.0.join("bin");
        assert_eq!(
            delete_folder_in(&t.0, "folder", true, |p| to_bin(p, &bin)),
            Ok(None)
        );
        assert_eq!(
            fs::read_to_string(t.0.join(PINNED)).unwrap(),
            "# Pins\n- [B](folder-other/a.md)\n- [Web](https://example.com/folder/a.md)\n"
        );
        assert_eq!(
            fs::read_to_string(bin.join("folder/sub/a.md")).unwrap(),
            "keep in bin"
        );
    }
    #[test]
    fn deleting_a_folder_blocks_late_saves_from_recreating_it() {
        let t = Temp::new();
        fs::create_dir(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/a.md"), "before").unwrap();
        let base = version(b"before");
        let bin = t.0.join("bin");
        delete_folder_in(&t.0, "folder", true, |p| to_bin(p, &bin)).unwrap();
        assert!(save_note_in(&t.0, "folder/a.md", "late save", &base).is_err());
        assert!(!t.0.join("folder").exists());
    }
    #[test]
    fn a_deleted_folder_can_be_explicitly_recreated_and_used() {
        let t = Temp::new();
        fs::create_dir(t.0.join("folder")).unwrap();
        delete_folder_in(&t.0, "folder", false, |_| unreachable!()).unwrap();
        create_folder_in(&t.0, "folder").unwrap();
        assert!(matches!(
            save_note_in(&t.0, "folder/a.md", "new", ""),
            Ok(Saved::Saved { .. })
        ));
    }
    #[test]
    fn confirmed_folder_that_became_empty_is_removed_without_recycling() {
        let t = Temp::new();
        fs::create_dir(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/a.md"), "removed externally").unwrap();
        assert_eq!(
            delete_folder_in(&t.0, "folder", false, |_| panic!("must confirm")),
            Ok(Some(FolderCounts { notes: 1, other: 0 }))
        );
        fs::remove_file(t.0.join("folder/a.md")).unwrap();
        assert_eq!(
            delete_folder_in(&t.0, "folder", true, |_| panic!("empty folder")),
            Ok(None)
        );
    }
    #[test]
    fn an_entry_arriving_before_empty_removal_returns_counts_and_survives() {
        let t = Temp::new();
        fs::write(t.0.join("late.md"), "keep").unwrap();
        assert_eq!(
            remove_empty_folder(&t.0),
            Ok(Some(FolderCounts { notes: 1, other: 0 }))
        );
        assert_eq!(fs::read_to_string(t.0.join("late.md")).unwrap(), "keep");
    }
    #[test]
    fn deleting_a_folder_refuses_root_outside_and_internal_paths() {
        let t = Temp::new();
        for path in [
            "",
            ".",
            "..",
            "../outside",
            "/",
            "C:/outside",
            "a/../../outside",
            ".hidden",
            "a/.hidden",
            "_pinned.md",
            "folder/.",
            "folder/",
            "folder//sub",
            "folder.",
            "folder ",
        ] {
            assert!(
                delete_folder_in(&t.0, path, true, |_| panic!("unsafe path")).is_err(),
                "{path}"
            );
        }
        assert!(t.0.is_dir());
    }
    #[test]
    fn deleting_a_folder_refuses_short_name_aliases() {
        use std::os::windows::process::CommandExt;
        let t = Temp::new();
        let target = t.0.join("Long folder name for alias");
        fs::create_dir(&target).unwrap();
        let output = std::process::Command::new("cmd")
            .args(["/D", "/C"])
            .raw_arg(format!(
                "for %I in (\"{}\") do @echo %~snxI",
                target.display()
            ))
            .output()
            .unwrap();
        assert!(output.status.success());
        let alias = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if alias == "Long folder name for alias" {
            eprintln!("8.3 alias unavailable on this volume; verify alias refusal manually");
            return;
        }
        assert!(
            t.0.join(&alias).is_dir(),
            "expected an existing short alias: {alias}"
        );
        assert_eq!(
            delete_folder_in(&t.0, &alias, true, |_| panic!("alias must not recycle")),
            Err(Error::InvalidPath)
        );
        assert!(target.is_dir());
    }
    #[test]
    fn folder_confirmation_counts_nested_notes_and_all_other_files() {
        let t = Temp::new();
        fs::create_dir_all(t.0.join("folder/sub")).unwrap();
        for file in ["a.md", "sub/b.MD", "sub/image.png", ".hidden"] {
            fs::write(t.0.join("folder").join(file), "keep").unwrap();
        }
        assert_eq!(
            delete_folder_in(&t.0, "folder", false, |_| panic!("must confirm")),
            Ok(Some(FolderCounts { notes: 2, other: 2 }))
        );
    }
    #[test]
    fn deleting_a_folder_refuses_junctions_at_every_level() {
        let t = Temp::new();
        fs::create_dir_all(t.0.join("real/sub")).unwrap();
        assert!(mklink(&["/J"], &t.0.join("link"), &t.0.join("real")));
        for path in ["link", "link/sub"] {
            assert_eq!(
                delete_folder_in(&t.0, path, true, |_| panic!("junction")),
                Err(Error::OutsideFolder)
            );
        }
        assert!(t.0.join("real/sub").is_dir());
    }

    /// Moves a deleted note into `bin`, as the Recycle Bin would.
    /// Moves `file` into `bin`, as the Recycle Bin would, and answers where it landed.
    fn to_bin(file: &Path, bin: &Path) -> io::Result<Option<PathBuf>> {
        fs::create_dir_all(bin)?;
        let landed = bin.join(file.file_name().unwrap());
        fs::rename(file, &landed)?;
        Ok(Some(landed))
    }
    fn delete(root: &Path, bin: &Path, path: &str) -> Result<bool, Error> {
        delete_note_in(root, path, |file| to_bin(file, bin))
    }
    /// Delete tests share the deleted notes, which hiding the panel clears for every folder.
    fn deleting() -> std::sync::MutexGuard<'static, ()> {
        static SERIAL: Mutex<()> = Mutex::new(());
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }
    #[test]
    fn undo_restores_what_the_recycle_bin_got_even_if_the_path_was_swapped() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "original").unwrap();
        let deleted = delete_note_in(&t.0, "a.md", |file| {
            // Another app moves the note away and puts a different file at its path.
            fs::rename(file, t.0.join("moved.md"))?;
            fs::write(file, "replacement")?;
            to_bin(file, &t.0.join("bin"))
        });
        assert_eq!(deleted, Ok(true));
        assert_eq!(undo_delete_in(&t.0, "a.md"), Ok(()));
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "replacement");
        assert_eq!(
            fs::read_to_string(t.0.join("moved.md")).unwrap(),
            "original"
        );
    }
    #[test]
    fn a_delete_finishing_after_the_panel_hid_keeps_nothing_for_undo() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "a").unwrap();
        let deleted = delete_note_in(&t.0, "a.md", |file| {
            forget_deleted();
            to_bin(file, &t.0.join("bin"))
        });
        assert_eq!(deleted, Ok(false));
        assert_eq!(undo_delete_in(&t.0, "a.md"), Err(Error::UndoUnavailable));
        assert!(!reserved(&t.0, "a.md"));
    }
    #[test]
    fn undoing_a_delete_brings_back_the_note_and_its_pinned_items() {
        let _serial = deleting();
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        fs::write(root.join("a.md"), "body\r\n").unwrap();
        let list = "# Mine\r\n- [A](a.md)\r\n- [C](c.md)\r\n";
        fs::write(root.join(PINNED), list).unwrap();
        assert_eq!(delete(&root, &t.0.join("bin"), "a.md"), Ok(true));
        assert!(!exists(&root.join("a.md")));
        assert_eq!(
            fs::read_to_string(root.join(PINNED)).unwrap(),
            "# Mine\r\n- [C](c.md)\r\n"
        );
        assert_eq!(undo_delete_in(&root, "a.md"), Ok(()));
        assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "body\r\n");
        assert_eq!(fs::read_to_string(root.join(PINNED)).unwrap(), list);
    }
    #[test]
    fn undoing_a_delete_never_overwrites_a_note_that_took_the_name() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "deleted").unwrap();
        assert_eq!(delete(&t.0, &t.0.join("bin"), "a.md"), Ok(true));
        fs::write(t.0.join("a.md"), "new").unwrap();
        assert_eq!(
            undo_delete_in(&t.0, "a.md"),
            Err(Error::RestoreTaken {
                path: "a.md".into()
            })
        );
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "new");
        // Once the name is free again, Undo still has the note.
        fs::remove_file(t.0.join("a.md")).unwrap();
        assert_eq!(undo_delete_in(&t.0, "a.md"), Ok(()));
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "deleted");
    }
    #[test]
    fn an_editor_saving_a_deleted_note_does_not_recreate_it() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "a").unwrap();
        assert_eq!(delete(&t.0, &t.0.join("bin"), "a.md"), Ok(true));
        for base in [version(b"a"), String::new()] {
            assert_eq!(
                save_note_in(&t.0, "A.md", "late edit", &base),
                Ok(Saved::Conflict)
            );
        }
        assert!(!exists(&t.0.join("a.md")));
    }
    #[test]
    fn renaming_cannot_take_the_name_of_a_deleted_note() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "a").unwrap();
        fs::write(t.0.join("b.md"), "b").unwrap();
        assert_eq!(delete(&t.0, &t.0.join("bin"), "a.md"), Ok(true));
        assert_eq!(rename(&t.0, "b.md", "a.md"), Err(Error::RenameConflict));
        assert!(exists(&t.0.join("b.md")));
    }
    #[test]
    fn a_note_the_recycle_bin_refuses_stays_with_its_pinned_items() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "a").unwrap();
        fs::write(t.0.join(PINNED), "- [A](a.md)\n").unwrap();
        let refused = delete_note_in(&t.0, "a.md", |_| Err(io::ErrorKind::Unsupported.into()));
        assert_eq!(refused, Err(Error::NotRecyclable));
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "a");
        assert_eq!(
            fs::read_to_string(t.0.join(PINNED)).unwrap(),
            "- [A](a.md)\n"
        );
        assert_eq!(undo_delete_in(&t.0, "a.md"), Err(Error::UndoUnavailable));
    }
    /// The trash entries left in `root`'s own trash.
    fn trashed(root: &Path) -> Vec<String> {
        fs::read_dir(root.join(trash::DIR)).map_or(Vec::new(), |entries| {
            entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
    }
    #[test]
    fn a_binless_delete_keeps_the_note_hidden_inside_the_folder_for_undo() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "secret").unwrap();
        fs::write(t.0.join("b.md"), "b").unwrap();
        assert_eq!(
            delete_note_in(&t.0, "a.md", |p| to_trash(&t.0, p)),
            Ok(true)
        );
        assert!(!exists(&t.0.join("a.md")));
        let kept = trashed(&t.0);
        assert!(kept.len() == 1 && kept[0].ends_with(" a.md"), "{kept:?}");
        let listed: Vec<_> = list(&t.0).unwrap().into_iter().map(|n| n.path).collect();
        assert_eq!(listed, ["b.md"]);
        assert_eq!(undo_delete_in(&t.0, "a.md"), Ok(()));
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "secret");
    }
    #[test]
    fn hiding_the_panel_purges_the_binless_trash() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "a").unwrap();
        assert_eq!(
            delete_note_in(&t.0, "a.md", |p| to_trash(&t.0, p)),
            Ok(true)
        );
        forget_deleted();
        // The purge runs on a thread of its own.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while exists(&t.0.join(trash::DIR)) && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!exists(&t.0.join(trash::DIR)));
        assert_eq!(undo_delete_in(&t.0, "a.md"), Err(Error::UndoUnavailable));
    }
    #[test]
    fn a_binless_delete_refuses_a_trash_that_is_a_link() {
        let _serial = deleting();
        let t = Temp::new();
        let (root, outside) = (t.0.join("notes"), t.0.join("outside"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        assert!(mklink(&["/J"], &root.join(trash::DIR), &outside));
        fs::write(root.join("a.md"), "a").unwrap();
        let deleted = delete_note_in(&root, "a.md", |p| to_trash(&root, p));
        assert!(
            matches!(deleted, Err(Error::DeleteFailed { .. })),
            "{deleted:?}"
        );
        assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "a");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        // Purging leaves a linked trash and what it points at alone.
        fs::write(outside.join("other.md"), "keep").unwrap();
        trash::purge(&root, u64::MAX);
        assert_eq!(
            fs::read_to_string(outside.join("other.md")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn purging_removes_links_in_the_trash_without_following_them() {
        let t = Temp::new();
        let (root, outside) = (t.0.join("notes"), t.0.join("outside"));
        fs::create_dir_all(root.join(trash::DIR).join("1-0-0 folder")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("other.md"), "keep").unwrap();
        assert!(mklink(
            &["/J"],
            &root.join(trash::DIR).join("1-0-1 link"),
            &outside
        ));
        assert!(mklink(
            &["/J"],
            &root.join(trash::DIR).join("1-0-0 folder").join("link"),
            &outside
        ));
        trash::purge(&root, u64::MAX);
        assert!(!exists(&root.join(trash::DIR)));
        assert_eq!(
            fs::read_to_string(outside.join("other.md")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn undoing_a_binless_folder_delete_brings_back_its_content_and_pins() {
        let _serial = deleting();
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        fs::create_dir_all(root.join("folder/attachments")).unwrap();
        fs::write(root.join("folder/a.md"), "a").unwrap();
        fs::write(root.join("folder/attachments/image.png"), "png").unwrap();
        let list = "- [A](folder/a.md)\n- [C](c.md)\n";
        fs::write(root.join(PINNED), list).unwrap();
        assert_eq!(
            delete_folder_in(&root, "folder", true, |p| to_trash(&root, p)),
            Ok(None)
        );
        assert!(!exists(&root.join("folder")));
        assert_eq!(
            fs::read_to_string(root.join(PINNED)).unwrap(),
            "- [C](c.md)\n"
        );
        assert_eq!(undo_delete_folder_in(&root, "folder"), Ok(()));
        assert_eq!(fs::read_to_string(root.join("folder/a.md")).unwrap(), "a");
        assert!(exists(&root.join("folder/attachments/image.png")));
        assert_eq!(fs::read_to_string(root.join(PINNED)).unwrap(), list);
        assert!(trashed(&root).is_empty());
    }
    #[test]
    fn undoing_a_folder_delete_never_merges_into_a_folder_that_took_the_name() {
        let _serial = deleting();
        let t = Temp::new();
        fs::create_dir(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/a.md"), "deleted").unwrap();
        delete_folder_in(&t.0, "folder", true, |p| to_trash(&t.0, p)).unwrap();
        fs::create_dir(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/a.md"), "new").unwrap();
        assert_eq!(
            undo_delete_folder_in(&t.0, "folder"),
            Err(Error::RestoreTaken {
                path: "folder".into()
            })
        );
        assert_eq!(fs::read_to_string(t.0.join("folder/a.md")).unwrap(), "new");
        fs::remove_dir_all(t.0.join("folder")).unwrap();
        assert_eq!(undo_delete_folder_in(&t.0, "folder"), Ok(()));
        assert_eq!(
            fs::read_to_string(t.0.join("folder/a.md")).unwrap(),
            "deleted"
        );
    }
    #[test]
    fn a_folder_in_the_recycle_bin_offers_no_undo() {
        let _serial = deleting();
        let t = Temp::new();
        fs::create_dir(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/a.md"), "a").unwrap();
        let bin = t.0.join("bin");
        delete_folder_in(&t.0, "folder", true, |p| to_bin(p, &bin)).unwrap();
        assert_eq!(
            undo_delete_folder_in(&t.0, "folder"),
            Err(Error::UndoUnavailable)
        );
        assert!(exists(&bin.join("folder/a.md")));
    }
    #[test]
    fn renaming_only_the_case_keeps_the_same_file() {
        let t = Temp::new();
        fs::write(t.0.join("note.md"), "text").unwrap();
        assert_eq!(rename(&t.0, "note.md", "Note.md"), Ok(()));
        let paths: Vec<_> = list(&t.0).unwrap().into_iter().map(|n| n.path).collect();
        assert_eq!(paths, ["Note.md"]);
    }
    #[test]
    fn renaming_stays_in_the_note_folder_and_away_from_the_pinned_list() {
        let t = Temp::new();
        fs::create_dir(t.0.join("sub")).unwrap();
        fs::write(t.0.join("a.md"), "a").unwrap();
        for to in ["sub/a.md", "../a.md", "_PINNED.md", "a.txt"] {
            assert_eq!(rename(&t.0, "a.md", to), Err(Error::InvalidPath), "{to}");
        }
        assert!(exists(&t.0.join("a.md")));
    }
    fn moving(root: &Path, from: &str, to: &str) -> Result<(), Error> {
        let base = version(&fs::read(root.join(from)).unwrap());
        move_note_in(root, from, to, &base)
    }
    #[test]
    fn moving_never_takes_a_name_in_the_target_folder() {
        let t = Temp::new();
        fs::create_dir(t.0.join("sub")).unwrap();
        fs::write(t.0.join("a.md"), "a").unwrap();
        fs::write(t.0.join("sub/A.md"), "b").unwrap();
        assert_eq!(moving(&t.0, "a.md", "sub/a.md"), Err(Error::RenameConflict));
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "a");
        assert_eq!(fs::read_to_string(t.0.join("sub/A.md")).unwrap(), "b");
    }
    #[test]
    fn moving_takes_the_pinned_items_along_and_keeps_their_text() {
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("a.md"), "body").unwrap();
        fs::write(root.join(PINNED), "- [My a](a.md)\n- [C](c.md)\n").unwrap();
        assert_eq!(moving(&root, "a.md", "sub/a.md"), Ok(()));
        assert!(!exists(&root.join("a.md")));
        assert_eq!(fs::read_to_string(root.join("sub/a.md")).unwrap(), "body");
        assert_eq!(
            fs::read_to_string(root.join(PINNED)).unwrap(),
            "- [My a](sub/a.md)\n- [C](c.md)\n"
        );
    }
    #[test]
    fn moving_only_reaches_visible_folders_that_exist_inside_the_notes_folder() {
        let t = Temp::new();
        let root = t.0.join("notes");
        let outside = t.0.join("outside");
        for dir in [&root, &outside, &root.join(".hidden")] {
            fs::create_dir(dir).unwrap();
        }
        assert!(mklink(&["/J"], &root.join("link"), &outside));
        fs::write(root.join("a.md"), "a").unwrap();
        for to in [
            "../a.md",
            ".hidden/a.md",
            "missing/a.md",
            "link/a.md",
            "A.md",
            PINNED,
        ] {
            assert!(moving(&root, "a.md", to).is_err(), "{to}");
        }
        assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "a");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    }
    #[test]
    fn moving_refuses_a_link_to_a_folder_inside_the_notes_folder() {
        let t = Temp::new();
        fs::create_dir(t.0.join("sub")).unwrap();
        fs::write(t.0.join("sub/a.md"), "a").unwrap();
        assert!(mklink(&["/J"], &t.0.join("dest"), &t.0));
        assert!(moving(&t.0, "sub/a.md", "dest/a.md").is_err());
        assert_eq!(fs::read_to_string(t.0.join("sub/a.md")).unwrap(), "a");
        assert!(!exists(&t.0.join("a.md")));
    }
    #[test]
    fn moving_through_a_link_cannot_take_the_name_of_a_deleted_note() {
        let _serial = deleting();
        let t = Temp::new();
        fs::create_dir(t.0.join("sub")).unwrap();
        fs::write(t.0.join("a.md"), "deleted").unwrap();
        fs::write(t.0.join("sub/a.md"), "a").unwrap();
        assert_eq!(delete(&t.0, &t.0.join("bin"), "a.md"), Ok(true));
        assert!(mklink(&["/J"], &t.0.join("dest"), &t.0));
        assert!(moving(&t.0, "sub/a.md", "dest/a.md").is_err());
        assert!(!exists(&t.0.join("a.md")));
        assert_eq!(undo_delete_in(&t.0, "a.md"), Ok(()));
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "deleted");
    }
    #[test]
    fn a_file_with_the_folder_name_is_reported_as_such() {
        let t = Temp::new();
        fs::write(t.0.join("Work"), "").unwrap();
        assert_eq!(
            create_folder_in(&t.0, "Work/Clients"),
            Err(Error::FolderNameTaken {
                name: "Work".into()
            })
        );
    }
    #[test]
    fn folder_names_stay_inside_and_are_kept_as_typed() {
        let t = Temp::new();
        for name in [
            "",
            "..",
            "../x",
            "a/../b",
            "C:/x",
            "/x",
            "a//b",
            "a\\b",
            ".git",
            "a.",
            "a ",
            " a",
            "a:b",
            "CON",
            "nul.txt",
            "COM1",
            "lpt9.x",
            "_pinned.md",
            "x\u{1}",
        ] {
            assert_eq!(
                create_folder_in(&t.0, name),
                Err(Error::InvalidFolderName),
                "{name:?}"
            );
        }
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 0);
        assert_eq!(create_folder_in(&t.0, "Projects/2026 Q1"), Ok(()));
        assert_eq!(create_folder_in(&t.0, "Projects"), Ok(()));
        assert!(t.0.join("Projects/2026 Q1").is_dir());
    }
    #[test]
    fn creating_a_folder_never_goes_through_a_link() {
        let t = Temp::new();
        let root = t.0.join("notes");
        let outside = t.0.join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        assert!(mklink(&["/J"], &root.join("link"), &outside));
        assert!(create_folder_in(&root, "link/x").is_err());
        assert!(!outside.join("x").exists());
    }
    #[test]
    fn folders_are_the_ones_listing_visits() {
        let t = Temp::new();
        let root = t.0.join("notes");
        for dir in ["notes/b", "notes/a/c", "notes/.h/x", "outside/d"] {
            fs::create_dir_all(t.0.join(dir)).unwrap();
        }
        assert!(mklink(&["/J"], &root.join("link"), &t.0.join("outside")));
        assert_eq!(folders_in(&root).unwrap(), ["a", "a/c", "b"]);
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
        // Older files have no side; an unknown side or width reads as the default and keeps the folder.
        fs::write(&p, r#"{"folder":"x","side":"top","width":"wide"}"#).unwrap();
        assert_eq!(load(&p).side, Side::Right);
        assert_eq!(load(&p).width, None);
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
