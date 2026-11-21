//! Local note storage and validation.
use crate::attachments;
use crate::dock::{Request, Side};
use crate::error::Error;
use crate::i18n::Locale;
use crate::pinned::{self, Kind};
use crate::platform::{self, hold_ancestry};
use crate::trash;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
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
    #[serde(default, deserialize_with = "sites_or_default")]
    allowed_sites: Vec<String>,
    /// Remembered answers to the page window's permission requests.
    #[serde(default, deserialize_with = "permissions_or_default")]
    site_permissions: Vec<crate::permissions::Remembered>,
    #[serde(default, deserialize_with = "lenient")]
    theme: Theme,
    #[serde(default, deserialize_with = "lenient")]
    search_engine: crate::web::SearchEngine,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "String::is_empty"
    )]
    custom_search_url: String,
    #[serde(default, deserialize_with = "lenient")]
    note_order: NoteOrder,
    #[serde(default, deserialize_with = "lenient")]
    group_by_date: bool,
    #[serde(default, deserialize_with = "lenient")]
    relative_dates: bool,
    #[serde(default, deserialize_with = "lenient")]
    note_search_suggestions: bool,
    /// Spell checking is on unless turned off.
    #[serde(default, deserialize_with = "lenient")]
    spelling_off: bool,
    /// Wins over `group_by_date`; two flags keep settings written before folder grouping valid.
    #[serde(default, deserialize_with = "lenient")]
    group_by_folder: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<PathBuf>,
    /// Optional Cryptomator entry point, a preference independent of note content.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    vault_file: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    side: Side,
    /// Panel width in logical pixels; absent for the default.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    width: Option<u32>,
    /// Activation shortcut as shown, such as `Ctrl+Alt+Space`; absent for the default.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    activation_shortcut: Option<String>,
    /// The id in the notes folder's marker (`MARKER`), recorded when it was chosen.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    folder_id: Option<String>,
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
/// A field that does not parse reads as its default: one bad or future value in `settings.json` must
/// not make the others, such as the notes folder, unreadable.
fn lenient<'de, D: Deserializer<'de>, T: serde::de::DeserializeOwned + Default>(
    value: D,
) -> Result<T, D::Error> {
    Ok(T::deserialize(serde_json::Value::deserialize(value)?).unwrap_or_default())
}
/// Keeps each valid entry on its own: one bad element must not erase the others on the next save,
/// and an edited entry that is not already a site key would silently widen or move its scope.
fn sites_or_default<'de, D: Deserializer<'de>>(value: D) -> Result<Vec<String>, D::Error> {
    let entries: Vec<serde_json::Value> = lenient(value)?;
    let mut sites = Vec::new();
    for entry in entries.iter().filter_map(serde_json::Value::as_str) {
        if crate::blocking::site_key(&format!("https://{entry}/")).as_deref() == Some(entry) {
            crate::blocking::change_allowed(&mut sites, entry, true);
        }
    }
    Ok(sites)
}
fn permissions_or_default<'de, D: Deserializer<'de>>(
    value: D,
) -> Result<Vec<crate::permissions::Remembered>, D::Error> {
    Ok(crate::permissions::from_settings(lenient(value)?))
}
pub fn site_permissions(app: &AppHandle) -> Vec<crate::permissions::Remembered> {
    config(app)
        .map(|p| load(&p).site_permissions)
        .unwrap_or_default()
}
pub fn set_site_permissions(
    app: &AppHandle,
    list: Vec<crate::permissions::Remembered>,
) -> Result<(), Error> {
    update(app, |settings| settings.site_permissions = list)
}
pub fn allowed_sites(app: &AppHandle) -> Vec<String> {
    config(app)
        .map(|p| load(&p).allowed_sites)
        .unwrap_or_default()
}
pub fn set_allowed_sites(app: &AppHandle, sites: Vec<String>) -> Result<(), Error> {
    update(app, |settings| settings.allowed_sites = sites)
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
pub fn note_search_suggestions(app: &AppHandle) -> Result<bool, Error> {
    Ok(load(&config(app)?).note_search_suggestions)
}
pub fn set_note_search_suggestions(app: &AppHandle, enabled: bool) -> Result<(), Error> {
    update(app, |settings| settings.note_search_suggestions = enabled)
}
pub fn spelling(app: &AppHandle) -> Result<bool, Error> {
    Ok(!load(&config(app)?).spelling_off)
}
pub fn set_spelling(app: &AppHandle, enabled: bool) -> Result<(), Error> {
    update(app, |settings| settings.spelling_off = !enabled)
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
#[derive(Serialize)]
pub struct Folder {
    path: String,
    state: Availability,
}
/// Whether the notes folder can be used. Neither `Unavailable` nor `Missing` is ever created: under a
/// locked vault's mount point, a new folder would put plaintext on the unencrypted disk.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Availability {
    Available,
    /// Its volume, share or a folder above it is gone or unreadable: a locked vault, an unplugged
    /// drive, an offline share.
    Unavailable,
    /// The folder above it is there but the folder is not: deleted, moved, or itself the mount point
    /// of a locked vault, which cannot be told apart.
    Missing,
    /// A folder is there but it is not the one chosen: its marker is absent or holds another id, as
    /// for the empty folder a locked vault can leave at its mount point.
    Unrecognized,
}
/// What looking at a path found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Probe {
    /// A folder that lists.
    Folder,
    /// Nothing, or a file, has the name.
    Absent,
    /// Any other failure, such as access denied or a network error.
    Unreadable,
}
/// A folder is `Missing` only when its parent is a folder that lists without it (`listed` is false);
/// whatever cannot be told is `Unavailable`. A volume root has no parent, so a drive or share that left
/// is unavailable, as is a name the parent lists that does not open, such as a link or mount point
/// whose target left.
fn availability(folder: Probe, parent: Option<Probe>, listed: bool) -> Availability {
    match (folder, parent) {
        (Probe::Folder, _) => Availability::Available,
        (Probe::Absent, Some(Probe::Folder)) if !listed => Availability::Missing,
        _ => Availability::Unavailable,
    }
}
/// Lists rather than reads attributes: an ACL can allow attributes but refuse listing.
fn probe(path: &Path) -> Probe {
    match fs::read_dir(path) {
        Ok(mut entries) => match entries.next() {
            Some(Err(_)) => Probe::Unreadable,
            _ => Probe::Folder,
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Probe::Absent,
        Err(_) if fs::metadata(path).is_ok_and(|m| !m.is_dir()) => Probe::Absent,
        Err(_) => Probe::Unreadable,
    }
}
/// Whether `parent` holds an entry called `name`; an entry that cannot be read counts as it.
fn lists(parent: &Path, name: &std::ffi::OsStr) -> bool {
    fs::read_dir(parent).map_or(true, |mut entries| {
        entries.any(|entry| {
            entry.map_or(true, |e| {
                same_name(Path::new(&e.file_name()), Path::new(name))
            })
        })
    })
}
fn folder_availability(path: &Path) -> Availability {
    match probe(path) {
        Probe::Folder => Availability::Available,
        folder => {
            let parent = path.parent();
            let listed = parent
                .zip(path.file_name())
                .is_none_or(|(parent, name)| lists(parent, name));
            availability(folder, parent.map(probe), listed)
        }
    }
}
/// Derived data lives in this hidden folder of the notes folder, which listing, search and the
/// watcher skip.
const DERIVED: &str = ".docdock";
/// The notes folder's identity marker in `DERIVED`: a random id, created when the folder is chosen and
/// recorded in `settings.json`. It syncs with the folder, so every computer sees the same one. A
/// folder behind the same path without it, such as the empty local folder a locked vault can leave at
/// its mount point, or another vault or drive, is never written to. Names no note.
const MARKER: &str = "folder-id";
fn new_folder_id() -> String {
    format!("{:016x}{:016x}", trash::random(), trash::random())
}
fn valid_folder_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// The id in `root`'s marker; `None` when there is none or it holds no valid id.
fn read_marker(root: &Path) -> io::Result<Option<String>> {
    let mut text = String::new();
    match fs::File::open(root.join(DERIVED).join(MARKER)) {
        // Any longer content is not an id; reading stops before a large file costs anything.
        Ok(file) => {
            if file.take(64).read_to_string(&mut text).is_err() {
                return Ok(None);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    }
    let id = text.trim();
    Ok(valid_folder_id(id).then(|| id.to_owned()))
}
/// `root`'s marker id, created if it has none or holds no valid one. Only for a folder the user
/// chose, or one `identity` trusts. Never creates `root`.
fn stamp(root: &Path) -> Result<String, Error> {
    if let Some(id) = read_marker(root)? {
        return Ok(id);
    }
    let (dir, _held) = held_derived_dir(root)?;
    let id = new_folder_id();
    write_atomic(&dir.join(MARKER), id.as_bytes())?;
    Ok(id)
}
/// What an available folder is, from the id the settings expect and the one its marker holds.
#[derive(Debug, PartialEq)]
enum Identity {
    /// The folder chosen.
    Chosen,
    /// Chosen before markers existed, and marked since, as by another computer: record its id.
    Adopt(String),
    /// Another folder, or, chosen before markers existed and unmarked, one that cannot be told from
    /// the folder a locked vault's mount point shows, which may even hold old plaintext notes.
    Foreign,
}
fn identity(expected: Option<&str>, marker: Option<&str>) -> Identity {
    match (expected, marker) {
        (Some(expected), Some(marker)) if expected == marker => Identity::Chosen,
        (Some(_), _) => Identity::Foreign,
        (None, Some(marker)) => Identity::Adopt(marker.to_owned()),
        (None, None) => Identity::Foreign,
    }
}
/// The notes folder at `path`, opened, if it is available and the one chosen (`expected`, the id in
/// `config`). Settings written before markers existed record one here, under `identity`'s rule.
fn open_chosen(
    config: &Path,
    path: &Path,
    expected: Option<&str>,
) -> Result<fs::File, Availability> {
    match folder_availability(path) {
        Availability::Available => {}
        state => return Err(state),
    }
    // Opened through links, on the volume that really holds the folder, before its marker is read.
    let held = platform::open_folder(path).map_err(|_| Availability::Unavailable)?;
    let marker = read_marker(path).map_err(|_| Availability::Unavailable)?;
    let id = match identity(expected, marker.as_deref()) {
        Identity::Chosen => return Ok(held),
        Identity::Foreign => return Err(Availability::Unrecognized),
        Identity::Adopt(id) => id,
    };
    // Decided again under the lock: another command may have recorded the id, or the folder changed.
    let _guard = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    let mut settings = load(config);
    if settings.folder.as_deref() != Some(path) {
        return Err(Availability::Unrecognized);
    }
    if let Some(recorded) = &settings.folder_id {
        let marker = read_marker(path).map_err(|_| Availability::Unavailable)?;
        return match marker {
            Some(marker) if marker == *recorded => Ok(held),
            _ => Err(Availability::Unrecognized),
        };
    }
    settings.folder_id = Some(id);
    save(config, &settings).map_err(|_| Availability::Unavailable)?;
    Ok(held)
}
/// The configured notes folder and, if it is available and the one chosen, its handle.
fn open_configured(config: &Path) -> Option<(PathBuf, Result<fs::File, Availability>)> {
    let settings = load(config);
    let path = settings.folder?;
    let opened = open_chosen(config, &path, settings.folder_id.as_deref());
    Some((path, opened))
}
/// The notes folder for one command. Its handle, opened where the marker was checked, stays open until
/// the command ends: the folder cannot be renamed or deleted meanwhile, and a vault asks before
/// locking under it.
struct Root {
    path: PathBuf,
    _held: fs::File,
}
impl std::ops::Deref for Root {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
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
#[cfg(test)]
fn read_settings(path: &Path) -> Option<PathBuf> {
    load(path).folder
}
pub fn search_engine(app: &AppHandle) -> crate::web::SearchEngine {
    config(app)
        .map(|p| load(&p).search_engine)
        .unwrap_or_default()
}
pub fn search_preferences(app: &AppHandle) -> crate::web::SearchPreferences {
    let settings = config(app).map(|p| load(&p)).unwrap_or_default();
    crate::web::SearchPreferences::new(settings.search_engine, settings.custom_search_url)
}

pub fn set_search_preferences(
    app: &AppHandle,
    engine: crate::web::SearchEngine,
    custom_url: String,
) -> Result<crate::web::SearchPreferences, Error> {
    update(app, |settings| {
        settings.search_engine = engine;
        settings.custom_search_url = custom_url.trim().to_owned();
    })?;
    Ok(search_preferences(app))
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
fn folder_in(config: &Path) -> Option<(PathBuf, Availability)> {
    let (path, opened) = open_configured(config)?;
    Some((
        path,
        opened.map_or_else(|state| state, |_| Availability::Available),
    ))
}
pub fn folder(app: &AppHandle) -> Result<Option<Folder>, Error> {
    Ok(folder_in(&config(app)?).map(|(path, state)| Folder {
        path: path.to_string_lossy().into_owned(),
        state,
    }))
}
/// The folder in use and whether it is available, for the watch that notices it leave and return.
pub fn folder_state(app: &AppHandle) -> Option<(PathBuf, Availability)> {
    folder_in(&config(app).ok()?)
}
fn save(path: &Path, settings: &Settings) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::unexpected("settings path has no folder"))?;
    fs::create_dir_all(parent)?;
    let mut value = serde_json::to_value(settings).map_err(Error::unexpected)?;
    // Preserve a pending legacy migration during preference changes while the vault is locked.
    if let Ok(bytes) = fs::read(path)
        && let Ok(old) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let Some(collapsed) = old.get("collapsed")
        && old.get("folder") == value.get("folder")
    {
        value["collapsed"] = collapsed.clone();
    }
    // Changing folders discards only obsolete UI state, never note content.
    let bytes = serde_json::to_vec(&value).map_err(Error::unexpected)?;
    Ok(write_atomic(path, &bytes)?)
}
/// Writes a complete temporary file next to `path` and renames it over `path` in one step. Any failure
/// leaves the previous file intact. Only for files the app owns, such as its settings, its caches and
/// what it derives in the notes folder; notes use `replace_note`, which keeps an external write.
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
/// reused or deleted: they may hold the only copy of a version. A temporary file's `<name>` is cut to
/// keep the whole within the name limit, so a note named near it can still be recreated; a backup's
/// is not, since recovering it reads the note's name from it.
fn sibling(path: &Path, suffix: &str) -> io::Result<PathBuf> {
    let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    loop {
        let n = NEXT_SIBLING.fetch_add(1, Ordering::Relaxed);
        let tail = format!(".{}-{n}.{suffix}", std::process::id());
        let mut unique = if suffix == TEMP_SUFFIX {
            platform::fit_name(name, platform::MAX_NAME.saturating_sub(tail.len()))
        } else {
            name.to_os_string()
        };
        unique.push(tail);
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
pub fn vault_file(app: &AppHandle) -> Result<Option<String>, Error> {
    Ok(load(&config(app)?).vault_file)
}
pub fn clear_vault_file(app: &AppHandle) -> Result<(), Error> {
    update(app, |settings| settings.vault_file = None)
}
/// A file dialog owned by the panel, which stays shown until the guard drops. Owned, the picker stays
/// above the panel and does not open a second picker.
fn dialog(
    app: &AppHandle,
    requests: &Sender<Request>,
) -> Result<(DialogGuard, rfd::FileDialog), Error> {
    requests
        .send(Request::SetDialog(true))
        .map_err(Error::unexpected)?;
    let guard = DialogGuard(requests.clone());
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| Error::unexpected("main window missing"))?;
    Ok((guard, rfd::FileDialog::new().set_parent(&window)))
}
pub fn choose_vault_file(
    app: &AppHandle,
    requests: &Sender<Request>,
) -> Result<Option<String>, Error> {
    let (_guard, dialog) = dialog(app, requests)?;
    if let Some(path) = dialog
        .add_filter("Cryptomator", &["cryptomator"])
        .pick_file()
    {
        validate_vault_file(&path)?;
        update(app, |settings| {
            settings.vault_file = Some(path.to_string_lossy().into_owned())
        })?;
    }
    vault_file(app)
}
/// Validate again at launch: the saved file can have been removed or replaced since selection.
fn validate_vault_file(path: &Path) -> Result<PathBuf, Error> {
    let extension = |path: &Path| {
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("cryptomator"))
    };
    if !path.is_absolute() || !extension(path) {
        return Err(Error::InvalidVaultFile);
    }
    let target = fs::canonicalize(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Error::VaultFileMissing
        } else {
            Error::from(error)
        }
    })?;
    if !extension(&target) || !target.is_file() {
        return Err(Error::InvalidVaultFile);
    }
    Ok(target)
}
pub fn unlock_vault(app: &AppHandle) -> Result<(), Error> {
    let file = vault_file(app)?.ok_or(Error::InvalidVaultFile)?;
    let target = validate_vault_file(Path::new(&file))?;
    platform::external::open_file(app, &target.to_string_lossy())
}
pub fn choose(app: &AppHandle, requests: &Sender<Request>) -> Result<Option<Folder>, Error> {
    let (_guard, dialog) = dialog(app, requests)?;
    if let Some(path) = dialog.pick_folder() {
        choose_in(&config(app)?, path)?;
        // Images pending from the folder before stay where they are, including those a send
        // already took: it finds the count changed once it holds the lock.
        let _saving = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        deleted().pending.clear();
        CHOSEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        crate::watch::stop();
        // The picker hid the preview already; this also ends a show validated in the old folder.
        crate::preview::close(app);
        purge_trash(app);
    }
    folder(app)
}
/// The user confirms that the folder at the configured path is the notes folder, as choosing it again
/// would, without leaving an editor whose text waits for it.
pub fn trust(app: &AppHandle) -> Result<Option<Folder>, Error> {
    trust_in(&config(app)?)?;
    folder(app)
}
fn trust_in(config: &Path) -> Result<(), Error> {
    let path = load(config).folder.ok_or(Error::NoFolder)?;
    match folder_availability(&path) {
        Availability::Available => choose_in(config, path),
        Availability::Missing => Err(Error::FolderMissing),
        _ => Err(Error::FolderUnavailable),
    }
}
/// Choosing is the trusted moment: the folder's marker is adopted, or created if it has none, and its
/// id recorded. Choosing again is how a folder whose marker changed, or a new folder at the same path,
/// is accepted. An unavailable folder is recorded without an id, and is never created.
fn choose_in(config: &Path, path: PathBuf) -> Result<(), Error> {
    let _guard = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    let id = match folder_availability(&path) {
        Availability::Available => Some(stamp(&path)?),
        _ => None,
    };
    let mut settings = load(config);
    settings.folder = Some(path);
    settings.folder_id = id;
    save(config, &settings)
}
/// The notes folder for a command, checked against its marker as each command starts. Legacy derived
/// data moves in once the folder is known to be the one chosen.
fn root_in(config: &Path) -> Result<Root, Error> {
    let (path, opened) = open_configured(config).ok_or(Error::NoFolder)?;
    let held = opened.map_err(|state| match state {
        Availability::Missing => Error::FolderMissing,
        Availability::Unrecognized => Error::FolderUnrecognized,
        _ => Error::FolderUnavailable,
    })?;
    {
        let _guard = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
        try_migrate_derived(&path, config);
    }
    Ok(Root { path, _held: held })
}
fn root(app: &AppHandle) -> Result<Root, Error> {
    root_in(&config(app)?)
}
pub(crate) fn markdown(path: &Path) -> bool {
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
/// Visits every file and folder below `dir`, each folder before what it holds, skipping hidden
/// folders; `visit` hears whether the entry is a folder. `is_dir` and `is_file` are false for symlinks
/// and junctions, so they are never followed. A subfolder that cannot be read, such as one without
/// permission or `System Volume Information` at a drive root, hides only what it holds: it is
/// answered instead of failing the walk. `dir` itself failing is an error.
fn walk(dir: &Path, visit: &mut impl FnMut(&fs::DirEntry, bool)) -> Result<Vec<PathBuf>, Error> {
    let mut skipped = Vec::new();
    walk_entries(fs::read_dir(dir)?, visit, &mut skipped)?;
    Ok(skipped)
}
fn walk_entries(
    entries: fs::ReadDir,
    visit: &mut impl FnMut(&fs::DirEntry, bool),
    skipped: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in entries {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_file() {
            visit(&entry, false);
        } else if ty.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
            let path = entry.path();
            match fs::read_dir(&path) {
                Ok(inner) => {
                    visit(&entry, true);
                    if walk_entries(inner, visit, skipped).is_err() {
                        skipped.push(path);
                    }
                }
                Err(_) => skipped.push(path),
            }
        }
    }
    Ok(())
}
/// Sorts `/` paths as the UI lists them: ignoring case, then exactly, so the order never depends on
/// the disk's.
fn sort_paths<T>(items: &mut [T], path: impl Fn(&T) -> &str) {
    items.sort_by_cached_key(|item| {
        let path = path(item);
        (path.to_lowercase(), path.to_owned())
    });
}
/// The path the UI uses: relative to `root`, with `/`.
pub(crate) fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
/// The notes below a folder, and the subfolders that could not be read.
#[derive(Serialize)]
pub struct Listing {
    notes: Vec<Note>,
    /// As `/` paths relative to the notes folder, sorted as notes are.
    skipped: Vec<String>,
}
/// The dates come from the listing itself: on Windows a directory entry carries them, so a large
/// folder costs no extra call per note.
fn list(root: &Path) -> Result<Listing, Error> {
    let mut notes = Vec::new();
    let skipped = walk(root, &mut |entry, folder| {
        let path = entry.path();
        if folder || !markdown(&path) {
            return;
        }
        let metadata = entry.metadata().ok();
        let modified = metadata.as_ref().and_then(|m| millis(m.modified()));
        notes.push(Note {
            path: relative(root, &path),
            name: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            created: metadata
                .and_then(|m| millis(m.created()))
                .or(modified)
                .unwrap_or(0),
            modified: modified.unwrap_or(0),
        });
    })?;
    sort_paths(&mut notes, |note| &note.path);
    let mut skipped: Vec<String> = skipped.iter().map(|path| relative(root, path)).collect();
    sort_paths(&mut skipped, |path| path);
    Ok(Listing { notes, skipped })
}
/// Listing is what the UI does on opening a folder, so it also starts watching it.
pub fn list_for(app: &AppHandle) -> Result<Listing, Error> {
    let root = root(app)?;
    crate::watch::ensure(app, &root);
    list(&root)
}
fn internal_path(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(DERIVED))
    })
}
/// A path the UI sent, before it touches the disk: relative, with `/`, only plain names and `allowed`.
fn checked(relative: &str, allowed: fn(&Path) -> bool) -> Result<&Path, Error> {
    let path = Path::new(relative);
    if internal_path(path)
        || relative.contains(':')
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
    resolve_below(&root.canonicalize()?, relative, allowed)
}
/// `resolve` with `root` already canonical, for a caller that resolves many paths.
fn resolve_below(
    root: &Path,
    relative: &str,
    allowed: fn(&Path) -> bool,
) -> Result<PathBuf, Error> {
    let path = checked(relative, allowed)?;
    let target = root.join(path).canonicalize()?;
    if !target.starts_with(root) || !target.is_file() || !allowed(&target) {
        return Err(Error::OutsideFolder);
    }
    if internal_path(
        target
            .strip_prefix(root)
            .map_err(|_| Error::OutsideFolder)?,
    ) {
        return Err(Error::InvalidPath);
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
    let Ok(root) = root.canonicalize() else {
        return Vec::new();
    };
    paths
        .iter()
        .filter_map(|relative| {
            let path = resolve_below(&root, relative, markdown).ok()?;
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
/// Counts the folders chosen, under `SAVING`, so work that started on the folder before a choice
/// can tell once it holds the lock.
static CHOSEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn folder_choices() -> u64 {
    CHOSEN.load(std::sync::atomic::Ordering::SeqCst)
}
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
    if let Err(error) = platform::move_new(&temp, &target) {
        // Still ours under the temp name; after a move, the name may be another save's.
        let _ = fs::remove_file(&temp);
        return if exists(&target) {
            Ok(Saved::Conflict)
        } else {
            Err(error.into())
        };
    }
    Ok(Saved::Saved {
        version: version(content),
    })
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
    edit_pinned(root, |text| pinned::rename(text, from, to, title)).map_err(|_| {
        eprintln!("[move] pinned items not moved");
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
    let root = root(app)?;
    let renamed = rename_note_in(&root, from, to, base, title);
    follow(&root, from, to, &renamed);
    renamed
}
pub fn move_note(app: &AppHandle, from: &str, to: &str, base: &str) -> Result<(), Error> {
    let root = root(app)?;
    let moved = move_note_in(&root, from, to, base);
    follow(&root, from, to, &moved);
    moved
}
/// Collapsed pinned state inside the notes folder follows a moved note.
fn follow(root: &Path, from: &str, to: &str, result: &Result<(), Error>) {
    if matches!(result, Ok(()) | Err(Error::PinnedNotRenamed { .. })) {
        // Best effort: the note has moved, and an error here would hide that from the editor, whose
        // next save would bring the old name back. A lost collapsed state only shows the card open.
        let _ = update_collapsed(root, |collapsed| {
            let before = collapsed.len();
            collapsed.retain(|target| target.to_lowercase() != from.to_lowercase());
            if collapsed.len() != before {
                collapsed.insert(to.to_owned());
            }
        });
    }
}
/// Every folder below `root` that listing visits, as `/` paths sorted as notes are: hidden folders,
/// links and folders that cannot be read are left out.
fn folders_in(root: &Path) -> Result<Vec<String>, Error> {
    let mut found = Vec::new();
    walk(root, &mut |entry, folder| {
        if folder {
            found.push(relative(root, &entry.path()));
        }
    })?;
    sort_paths(&mut found, |path| path);
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
    name.chars().count() <= 100 && !name.starts_with(['.', ' ']) && windows_name(name)
}
/// A name Windows keeps as written: no reserved character, no trailing dot or space, no device name
/// such as `con` or `com1`, even with an extension.
fn windows_name(name: &str) -> bool {
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
/// Where a deleted note or folder went.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Removal {
    /// `undo_delete` can bring the note back from the Recycle Bin, until the panel hides.
    undo: bool,
    /// The entry of the notes folder's own trash that holds it, which `restore_trashed` brings back.
    trash: Option<String>,
    /// How many images only the note referenced follow it after the panel hides (`Pending`).
    images: usize,
}
/// Where `send_away` put an item.
enum Gone {
    /// The Recycle Bin, and where the item landed there when the shell says.
    Bin(Option<PathBuf>),
    /// The notes folder's own trash, under this entry id.
    Trash(String),
}
/// Sends `target` to the Recycle Bin through `recycle`, or to the notes folder's own trash where the
/// Recycle Bin cannot take it, as on a vault mounted by Cryptomator. Called under the save lock, with
/// `target`'s parents held.
fn send_away(
    root: &Path,
    target: &Path,
    recycle: impl Fn(&Path) -> io::Result<Option<PathBuf>>,
) -> Result<Gone, Error> {
    let failed = |error: io::Error| Error::DeleteFailed {
        detail: error.to_string(),
    };
    match recycle(target) {
        Ok(landed) => Ok(Gone::Bin(landed)),
        Err(error) if error.kind() == io::ErrorKind::Unsupported => {
            let canonical = root.canonicalize()?;
            let real = target.canonicalize()?;
            let relative = real
                .strip_prefix(&canonical)
                .map_err(|_| Error::OutsideFolder)?;
            let (dir, _held) = trash_dir(&canonical)?;
            trash::put(&dir, relative, target)
                .map(Gone::Trash)
                .map_err(failed)
        }
        Err(error) => Err(failed(error)),
    }
}
/// The notes folder's own trash, created if needed while `.docdock` is held, and held with every
/// folder above it until the handles drop. Never creates `root`; a link in its place is refused.
fn trash_dir(root: &Path) -> Result<(PathBuf, Vec<fs::File>), Error> {
    let (derived, mut held) = held_derived_dir(root)?;
    let dir = derived.join(trash::DIR);
    match fs::create_dir(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let trash = platform::hold_directory(&dir).map_err(|error| Error::DeleteFailed {
        detail: error.to_string(),
    })?;
    held.push(trash);
    Ok((dir, held))
}
/// Removes the pinned items of `path`, a deleted note or folder, with `unpin`, and remembers them so
/// Undo can bring them back with the item. If the list cannot change, the answer is
/// `PinnedNotRemoved`: the item is deleted all the same.
fn unpin_deleted(
    root: &Path,
    path: &str,
    folder: bool,
    removal: &Removal,
    unpin: impl Fn(&str) -> String,
) -> Result<(), Error> {
    let change = std::cell::RefCell::new(None);
    edit_pinned(root, |text| {
        let edited = unpin(text);
        *change.borrow_mut() = (edited != text).then(|| (text.to_owned(), edited.clone()));
        edited
    })
    .map_err(|_| {
        eprintln!("[delete] pinned items not removed");
        Error::PinnedNotRemoved {
            undo: removal.undo,
            trash: removal.trash.clone(),
            images: removal.images,
        }
    })?;
    let Some(change) = change.into_inner() else {
        return Ok(());
    };
    let mut state = deleted();
    if let Some(id) = &removal.trash {
        state.trashed.push(Trashed {
            root: root.to_owned(),
            id: id.clone(),
            folder,
            pinned: change,
        });
    } else if let Some(entry) = state
        .notes
        .iter_mut()
        .find(|d| d.root == root && d.path == path)
    {
        entry.pinned = Some(change);
    }
    Ok(())
}
/// Deletes a folder and everything in it at once: an empty one is removed, any other goes to the
/// Recycle Bin, or the notes folder's own trash where the Recycle Bin cannot take it. An entry arriving
/// after the empty check is never deleted for good: the folder goes away whole instead.
fn delete_folder_in(
    root: &Path,
    relative: &str,
    recycle: impl Fn(&Path) -> io::Result<Option<PathBuf>>,
) -> Result<Removal, Error> {
    if !relative.split('/').all(|name| {
        !name.is_empty()
            && !name.starts_with('.')
            && !name.ends_with(['.', ' '])
            && !name.eq_ignore_ascii_case(PINNED)
    }) {
        return Err(Error::InvalidPath);
    }
    let path = checked(relative, |_| true)?;
    let removal = {
        let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        let (target, mut held) = hold_folder(root, path)?;
        // Refuse short-name aliases too: pins must name the folder being deleted.
        if !same_name(&target, &target.canonicalize()?) {
            return Err(Error::InvalidPath);
        }
        let empty = fs::read_dir(&target)?.next().transpose()?.is_none();
        // Keep all parents held; the target itself must allow the shell or the trash to move it.
        held.pop();
        let removed = empty
            && match fs::remove_dir(&target) {
                Ok(()) => true,
                Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => false,
                Err(error) => return Err(error.into()),
            };
        let trash = if removed {
            None
        } else {
            match send_away(root, &target, recycle)? {
                Gone::Trash(id) => Some(id),
                Gone::Bin(_) => None,
            }
        };
        deleted()
            .notes
            .retain(|d| d.root != root || !inside_folder(&d.path, relative));
        Removal {
            trash,
            ..Removal::default()
        }
    };
    unpin_deleted(root, relative, true, &removal, |text| {
        pinned::parse(text)
            .iter()
            .filter(|item| item.kind == Kind::Note && inside_folder(&item.target, relative))
            .fold(text.to_owned(), |text, item| {
                pinned::unpin(&text, &item.target)
            })
    })?;
    Ok(removal)
}
pub fn delete_folder(app: &AppHandle, path: &str) -> Result<Removal, Error> {
    delete_folder_in(&root(app)?, path, platform::recycle)
}
/// Puts the pinned items of `relative`, a note or a folder, back as they were before its delete
/// changed `_pinned.md` from `before` to `after`: in place if the list did not change since. A deleted
/// list is left for its own restore, rather than created with only these items. Each item comes back
/// once.
fn repin(
    root: &Path,
    relative: &str,
    folder: bool,
    before: &str,
    after: &str,
) -> Result<(), Error> {
    if !exists(&root.join(PINNED)) {
        return Ok(());
    }
    let targets: Vec<String> = if folder {
        pinned::parse(before)
            .into_iter()
            .filter(|item| item.kind == Kind::Note && inside_folder(&item.target, relative))
            .map(|item| item.target)
            .collect()
    } else {
        vec![relative.to_owned()]
    };
    edit_pinned(root, |text| {
        if text == after {
            return before.to_owned();
        }
        targets.iter().fold(text.to_owned(), |text, target| {
            pinned::repin(&text, before, after, target)
        })
    })
    .map_err(|_| {
        eprintln!("[undo] pinned items not restored");
        Error::PinnedNotRestored
    })
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
/// An item deleted to the notes folder's own trash whose delete removed pinned items, which Undo
/// brings back with it. The entry itself is on disk and outlives the panel session.
struct Trashed {
    root: PathBuf,
    id: String,
    folder: bool,
    /// `_pinned.md` before and after the item's pinned items were removed.
    pinned: (String, String),
}
/// The images in `attachments/` folders that only a deleted note referenced, which follow it once its
/// panel session ended and the UI saved what it left open (`send_pending`). Until then they stay where
/// they are, so an Undo needs nothing for them; only their paths are kept, never their bytes.
struct Pending {
    root: PathBuf,
    /// The folder's marker id at the delete: the images go only from that very folder.
    folder_id: String,
    /// The panel session of the delete, which must have ended before the images go.
    session: u64,
    /// The note's path, which a Recycle Bin Undo names.
    note: String,
    images: Vec<String>,
    /// The trash entry that holds the note, when it went to the notes folder's own trash.
    trash: Option<String>,
}
/// What Undo needs that the disk does not keep. `session` counts the panel hides, so a delete to the
/// Recycle Bin that finishes after one keeps nothing for Undo. Locked after `SAVING` when both are
/// needed, never the other way.
struct Trash {
    session: u64,
    notes: Vec<Deleted>,
    trashed: Vec<Trashed>,
    pending: Vec<Pending>,
}
static DELETED: Mutex<Trash> = Mutex::new(Trash {
    session: 0,
    notes: Vec::new(),
    trashed: Vec::new(),
    pending: Vec::new(),
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
/// app swapped the note's path meanwhile; it is possible unless the panel hid during the delete or the
/// Recycle Bin copy cannot be read. Where the Recycle Bin cannot take the note, it goes to the notes
/// folder's own trash instead. Its pinned items are removed after; if they cannot be, the answer is
/// `PinnedNotRemoved`: the note is deleted all the same. A note too large to keep a copy of for Undo
/// is refused where `recyclable` says the Recycle Bin would take it (or cannot tell); the trash needs
/// no copy. The images in `attachments` folders that only this note references stay for now: they are
/// pending until the panel session ends (`Pending`), and `images` counts them; a folder without a
/// marker id keeps them.
fn delete_note_in(
    root: &Path,
    relative: &str,
    recycle: impl Fn(&Path) -> io::Result<Option<PathBuf>>,
    recyclable: impl Fn(&Path) -> io::Result<bool>,
) -> Result<Removal, Error> {
    if checked(relative, markdown)?
        .as_os_str()
        .eq_ignore_ascii_case(PINNED)
    {
        return Err(Error::InvalidPath);
    }
    let session = deleted().session;
    let removal = {
        let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        let target = resolve(root, relative, markdown)?;
        let _held = hold_ancestry(target.parent().ok_or(Error::InvalidPath)?)?;
        let size = fs::metadata(&target)?.len();
        if too_large(size).is_err() && recyclable(&target).unwrap_or(true) {
            too_large(size)?;
        }
        let canonical = root.canonicalize()?;
        let mine = images_of(&canonical, &target).unwrap_or_else(|error| {
            eprintln!("[delete] images not checked: {error:?}");
            Vec::new()
        });
        let removal = match send_away(root, &target, recycle)? {
            Gone::Trash(id) => Removal {
                trash: Some(id),
                ..Removal::default()
            },
            Gone::Bin(landed) => {
                let bytes = landed.and_then(|bin| read_bytes(&bin).ok());
                let mut trash = deleted();
                let undo = match bytes {
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
                };
                Removal {
                    undo,
                    ..Removal::default()
                }
            }
        };
        // Of those, the ones no other note references, live or in the trash. Any doubt keeps them all.
        let selection = mine.iter().map(String::as_str).collect();
        let folder_id = read_marker(root).ok().flatten();
        let images = match &folder_id {
            Some(_) if !mine.is_empty() => {
                unused_images_in(&canonical, Some(&selection), removal.trash.as_deref())
                    .unwrap_or_else(|error| {
                        eprintln!("[delete] images kept: {error:?}");
                        Vec::new()
                    })
            }
            _ => Vec::new(),
        };
        let count = images.len();
        if let Some(folder_id) = folder_id.filter(|_| count > 0) {
            deleted().pending.push(Pending {
                root: root.to_owned(),
                folder_id,
                session,
                note: relative.to_owned(),
                images,
                trash: removal.trash.clone(),
            });
        }
        Removal {
            images: count,
            ..removal
        }
    };
    unpin_deleted(root, relative, false, &removal, |text| {
        pinned::unpin(text, relative)
    })?;
    Ok(removal)
}
/// Writes a note deleted to the Recycle Bin back at its path, never over a file that took the name
/// since (`RestoreTaken`, and Undo stays possible). Its pinned items return too. Its images never
/// left: they are no longer pending.
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
        let pinned = deleted.remove(at).pinned;
        trash
            .pending
            .retain(|p| p.root != root || p.trash.is_some() || p.note != relative);
        pinned
    };
    match pinned {
        Some((before, after)) => repin(root, relative, false, &before, &after),
        None => Ok(()),
    }
}
/// Moves `source` to `path`, an image's path below the canonical `root`, recreating the folders above
/// it. Only under that very name: notes link an image by its name, so a free name such as
/// `map (2).png` would leave the link broken while the image looked restored. A name taken, a link, a
/// file in a folder's place or a path that is no image in an `attachments` folder fails, and `source`
/// stays where it is.
fn place_image(root: &Path, source: &Path, path: &str) -> Result<(), Error> {
    let image = checked(path, attachment_path)?;
    if !image
        .iter()
        .all(|name| name.to_str().is_some_and(windows_name))
    {
        return Err(Error::InvalidPath);
    }
    let parent = image.parent().ok_or(Error::InvalidPath)?;
    let name = image.file_name().ok_or(Error::InvalidPath)?;
    let _held = ensure_folders(root, parent)?;
    let into = root.join(parent);
    place(source, &into, name, false, std::iter::empty(), |_| false).map(|_| ())
}
pub fn delete_note(app: &AppHandle, path: &str) -> Result<Removal, Error> {
    delete_note_in(&root(app)?, path, platform::recycle, platform::recyclable)
}
pub fn undo_delete(app: &AppHandle, path: &str) -> Result<(), Error> {
    undo_delete_in(&root(app)?, path)
}
/// The panel closed: deleted notes stay in the Recycle Bin and their names are free again. Items in
/// the notes folder's own trash stay there, with what their Undo needs. The images pending wait for
/// `send_pending`. Answers the session that ended.
pub fn forget_deleted() -> u64 {
    let mut trash = deleted();
    trash.session += 1;
    trash.notes.clear();
    trash.session - 1
}
/// The notes folder went away, such as a vault that locked: nothing derived from its notes stays in memory,
/// including the pinned items kept for a trash item's Undo (decision 0009) and the images pending,
/// which then stay where they are. The items stay in the trash; only their Undo no longer puts pinned
/// items back.
pub fn forget_unavailable() {
    let _ = forget_deleted();
    let mut trash = deleted();
    trash.trashed.clear();
    trash.pending.clear();
}
/// The UI saved what an ended panel session left open, such as a note that just gained a link to a
/// pending image: the images only the notes deleted in ended sessions referenced follow them
/// (`send_pending_in`). Only those of `session`, the one whose hide the UI saved after, and before:
/// a late request must not take the images of a later session whose editor is still saving. Answers
/// whether any were pending.
pub fn send_pending(app: &AppHandle, session: u64) -> Result<bool, Error> {
    let chosen = folder_choices();
    let root = root(app)?;
    let ended = ended_pending(session);
    let any = !ended.is_empty();
    send_pending_in(&root, ended, chosen, platform::recycle);
    Ok(any)
}
/// Takes the images pending from notes deleted in panel sessions up to `session`, if it ended.
fn ended_pending(session: u64) -> Vec<Pending> {
    let mut trash = deleted();
    let ended = session.min(trash.session.saturating_sub(1));
    if trash.session == 0 {
        return Vec::new();
    }
    let (ended, current) = std::mem::take(&mut trash.pending)
        .into_iter()
        .partition(|p| p.session <= ended);
    trash.pending = current;
    ended
}
/// Sends away the images `pending` for notes deleted in `root`, those no note references now, live or
/// in the trash, but the deleted note in its own entry (`send_images_in`), if `root` is still the
/// folder each was recorded in, by path and marker id. Where the note went to the notes folder's own
/// trash, they join its entry, so restoring it brings them back. Any doubt or error keeps an image
/// where it is, only logged: Preferences then lists it among the unused images.
fn send_pending_in(
    root: &Path,
    pending: Vec<Pending>,
    chosen: u64,
    recycle: impl Fn(&Path) -> io::Result<Option<PathBuf>>,
) {
    for pending in pending.into_iter().filter(|p| p.root == root) {
        let entry = pending.trash.as_deref();
        let folder = (pending.folder_id.as_str(), chosen);
        let sent = send_images_in(root, &pending.images, entry, folder, &recycle);
        if let Err(error) = sent {
            eprintln!("[delete] images kept: {error:?}");
        }
    }
}
/// The trash's place in `root`, which may not exist.
fn trash_path(root: &Path) -> Result<PathBuf, Error> {
    Ok(root.canonicalize()?.join(DERIVED).join(trash::DIR))
}
/// What a failure to hold or read the trash `dir` means: a link where the trash or the derived-data
/// folder should be is refused; anything else is the folder going away, such as a vault locking.
fn trash_error(dir: &Path, error: io::Error) -> Error {
    let linked = [dir, dir.parent().unwrap_or(dir)]
        .iter()
        .any(|path| fs::symlink_metadata(path).is_ok_and(|m| m.is_symlink()));
    if linked {
        Error::OutsideFolder
    } else if error.kind() == io::ErrorKind::NotFound {
        Error::TrashItemMissing
    } else {
        Error::FolderUnavailable
    }
}
/// The entries in the notes folder's own trash, newest first.
fn trash_items_in(root: &Path) -> Result<Vec<trash::Item>, Error> {
    let dir = trash_path(root)?;
    trash::list(&dir).map_err(|error| trash_error(&dir, error))
}
pub fn trash_items(app: &AppHandle) -> Result<Vec<trash::Item>, Error> {
    trash_items_in(&root(app)?)
}
/// Moves the item of trash entry `id` back to its path, recreating the folders above it that are gone,
/// and answers the path it took. A name taken since gives a free one, such as `name (2).md`: nothing
/// is ever overwritten. `undo`: also put back the pinned items its delete removed, when it returns
/// under its own name. `TrashItemMissing` when the entry is gone, such as restored or emptied
/// elsewhere.
///
/// The images deleted with a note come back with it, each to its own path, so the note's links find
/// them again; the note's own new name, if any, is in the same folder, where its relative links still
/// hold. An image returns only under its own name (`place_image`): one whose name is taken, or that
/// cannot move, such as one another app holds open, stays in the entry, which then holds only images
/// and lists and restores as any item; the answer is `ImagesLeftInTrash`: the note is back all the
/// same. Restoring an entry with only images answers the first image's path.
fn restore_trashed_in(root: &Path, id: &str, undo: bool) -> Result<String, Error> {
    let (item, restored, left) = {
        let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        let canonical = root.canonicalize()?;
        let dir = trash_path(&canonical)?;
        let _held_trash = hold_ancestry(&dir).map_err(|error| trash_error(&dir, error))?;
        // The entry's folders stay held from the moment they are read until the item has moved.
        let found = trash::find(&dir, id).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => Error::TrashItemMissing,
            io::ErrorKind::InvalidData => Error::InvalidPath,
            _ => trash_error(&dir, error),
        })?;
        let (folder, held_source) = (found.folder, found.held);
        let item = (found.path, folder);
        let path = checked(&item.0, |_| true)?;
        let name = path.file_name().ok_or(Error::InvalidPath)?;
        let parent = path.parent().unwrap_or(Path::new(""));
        let allowed = name.to_str().is_some_and(|name| {
            windows_name(name)
                && if folder {
                    !name.starts_with('.')
                } else {
                    markdown(path) || attachment_path(path)
                }
        });
        // An entry with only images checks each as it places it, as the images of a note.
        if found.source.is_some() && (!allowed || path.as_os_str().eq_ignore_ascii_case(PINNED)) {
            return Err(Error::InvalidPath);
        }
        let held_parent = match found.source {
            Some(_) => ensure_folders(&canonical, parent)?,
            None => Vec::new(),
        };
        let mut left = Vec::new();
        let (landed, moved) = {
            let _held = (held_parent, held_source);
            let landed = match &found.source {
                Some(source) => Some(place(
                    source,
                    &canonical.join(parent),
                    name,
                    folder,
                    (2..=100).map(|n| format!(" ({n})")),
                    |candidate| !folder && reserved(root, &relative(&canonical, candidate)),
                )?),
                None => None,
            };
            let images = found.images.len();
            for (source, image) in found.images {
                // Each on its own: one that cannot return, or a file that is no image, such as one
                // another app left there, stays in this entry and never holds back the rest.
                if let Err(error) = place_image(&canonical, &source, &image) {
                    eprintln!("[restore] image stays in the trash: {error:?}");
                    left.push(image);
                }
            }
            let moved = landed.is_some() || left.len() < images;
            (landed, moved)
        };
        // Once nothing holds the entry's folders.
        trash::discard_entry(&dir, id);
        // What stays of a partial restore is a new item: its 30 days start again, under a name dated
        // now, since the entry's folder may not show the change.
        if moved
            && !left.is_empty()
            && let Err(error) = trash::renew(&dir, id)
        {
            eprintln!("[restore] entry not renewed: {error:?}");
        }
        let restored =
            landed.map_or_else(|| item.0.clone(), |landed| relative(&canonical, &landed));
        (item, restored, left)
    };
    let pinned = {
        let mut state = deleted();
        let at = state
            .trashed
            .iter()
            .position(|t| t.root == root && t.id == id);
        // Its images never left if they were still pending.
        state
            .pending
            .retain(|p| p.root != root || p.trash.as_deref() != Some(id));
        at.map(|at| state.trashed.remove(at))
    };
    let repinned = match pinned {
        Some(trashed) if undo && restored == item.0 => {
            let (before, after) = &trashed.pinned;
            repin(root, &item.0, trashed.folder, before, after)
        }
        _ => Ok(()),
    };
    if left.is_empty() {
        return repinned.map(|()| restored);
    }
    if repinned.is_err() {
        eprintln!("[restore] pinned items not restored either");
    }
    Err(Error::ImagesLeftInTrash {
        path: restored,
        count: left.len(),
        images: left.join(", "),
    })
}
pub fn restore_trashed(app: &AppHandle, id: &str, undo: bool) -> Result<String, Error> {
    restore_trashed_in(&root(app)?, id, undo)
}
/// Makes sure each folder of `relative` exists below `root`, creating the missing ones. Only a folder
/// to create must pass the folder name rules: one that exists, even under a name the app would not
/// give, is used as it is. Links are refused at every level. Each folder is created while the one
/// above it is held, and every level stays held in the handles answered.
fn ensure_folders(root: &Path, relative: &Path) -> Result<Vec<fs::File>, Error> {
    let mut held = hold_ancestry(root).map_err(|_| Error::OutsideFolder)?;
    let mut dir = root.to_owned();
    for name in relative.iter() {
        dir.push(name);
        match fs::symlink_metadata(&dir) {
            Ok(metadata) if metadata.is_symlink() => return Err(Error::OutsideFolder),
            Ok(metadata) if metadata.is_dir() => {
                held.push(platform::hold_directory(&dir).map_err(|_| Error::OutsideFolder)?);
                continue;
            }
            Ok(_) => {
                return Err(Error::FolderNameTaken {
                    name: name.to_string_lossy().into_owned(),
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if !name.to_str().is_some_and(folder_name_allowed) {
            return Err(Error::InvalidFolderName);
        }
        match fs::create_dir(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        held.push(platform::hold_directory(&dir).map_err(|_| Error::OutsideFolder)?);
    }
    Ok(held)
}
/// Moves `source` into `folder` as `name`, never over anything. If that is taken, the first free name
/// among those `numbered` gives, put between stem and extension, such as `name (2).md`
/// or `folder (2)`; the stem is cut so the whole fits the name limit. `taken` refuses a name besides
/// those on disk, such as one a Recycle Bin Undo reserves.
fn place(
    source: &Path,
    folder: &Path,
    name: &std::ffi::OsStr,
    is_folder: bool,
    numbered: impl Iterator<Item = String>,
    taken: impl Fn(&Path) -> bool,
) -> Result<PathBuf, Error> {
    let file = Path::new(name);
    let (stem, extension) = match (is_folder, file.file_stem(), file.extension()) {
        (false, Some(stem), Some(extension)) => (stem, format!(".{}", extension.to_string_lossy())),
        _ => (name, String::new()),
    };
    for suffix in std::iter::once(None).chain(numbered.map(Some)) {
        let candidate = if let Some(suffix) = suffix {
            let tail = format!("{suffix}{extension}");
            let mut numbered =
                platform::fit_name(stem, platform::MAX_NAME.saturating_sub(tail.len()));
            numbered.push(tail);
            folder.join(numbered)
        } else {
            folder.join(name)
        };
        if taken(&candidate) {
            continue;
        }
        match platform::move_new(source, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(_) if exists(&candidate) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(Error::unexpected("no free name"))
}
/// Deletes for good the entries `ids` of the notes folder's own trash: those the user saw listed. Under
/// the save lock, as a restore, so an entry never goes while a restore moves its note and images.
fn empty_trash_in(root: &Path, ids: &[String]) -> Result<(), Error> {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let dir = trash_path(root)?;
    let result = trash::empty(&dir, ids);
    // An entry that could not be deleted keeps the pinned items its Undo puts back.
    deleted()
        .trashed
        .retain(|t| t.root != root || !ids.contains(&t.id) || exists(&dir.join(&t.id)));
    result.map_err(|error| Error::EmptyFailed {
        detail: error.to_string(),
    })
}
pub fn empty_trash(app: &AppHandle, ids: &[String]) -> Result<(), Error> {
    empty_trash_in(&root(app)?, ids)
}
/// Deletes for good, away from the caller's thread, what has been in the notes folder's own trash
/// longer than `trash::KEEP_MS`: at startup, on choosing a folder and when the panel hides. Only in
/// the folder chosen, checked by its marker, and never while it is unavailable. Under the save lock,
/// as `empty_trash_in`.
pub fn purge_trash(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(root) = root(&app) else { return };
        let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(dir) = trash_path(&root) {
            trash::purge(&dir, trash::now_ms());
        }
    });
}
/// The pinned items live in this note at the root of the folder (decision 0005).
const PINNED: &str = "_pinned.md";
/// In `DERIVED`: the last version of `_pinned.md` read, to restore it if it is deleted. Before it
/// moved there, it sat under the same name beside `settings.json`.
const PINNED_COPY_FILE: &str = "pinned-copy.json";
/// In `DERIVED`: the pinned items shown as one line. It once was the `collapsed` key of the settings.
const COLLAPSED_FILE: &str = "collapsed.json";
/// A read, a line edit and a write that keep losing to external writes give up after this many tries.
const PINNED_TRIES: usize = 3;

#[derive(Serialize)]
pub struct PinnedItem {
    #[serde(flatten)]
    item: pinned::Item,
    /// A note that no longer exists at that path; the UI offers to remove the item.
    missing: bool,
    /// Shown as one line in this notes folder.
    collapsed: bool,
}
#[derive(Serialize)]
pub struct Pinned {
    version: String,
    items: Vec<PinnedItem>,
    /// `_pinned.md` is gone but a copy of its last read version can bring it back.
    restorable: bool,
}
/// The last version of `_pinned.md`, kept inside the notes folder for restoration.
#[derive(Serialize, Deserialize)]
struct PinnedCopy {
    folder: PathBuf,
    text: String,
}
/// The derived-data folder, hidden from listing, search and watcher; never creates the notes root or
/// follows a link. It is created while `root` is held, and held with every folder above it until the
/// handles drop: callers keep them until their writes end, so nothing lands elsewhere.
fn held_derived_dir(root: &Path) -> Result<(PathBuf, Vec<fs::File>), Error> {
    let mut held = hold_ancestry(root)?;
    let dir = root.join(DERIVED);
    match fs::create_dir(&dir) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.into()),
    }
    held.push(platform::hold_directory(&dir)?);
    Ok((dir, held))
}
fn read_collapsed(dir: &Path) -> Result<BTreeSet<String>, Error> {
    match fs::read(dir.join(COLLAPSED_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(Error::unexpected),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(error) => Err(error.into()),
    }
}
static COLLAPSED: Mutex<()> = Mutex::new(());
fn update_collapsed(root: &Path, change: impl FnOnce(&mut BTreeSet<String>)) -> Result<(), Error> {
    let _guard = COLLAPSED.lock().unwrap_or_else(|e| e.into_inner());
    let (dir, _held) = held_derived_dir(root)?;
    let mut collapsed = read_collapsed(&dir)?;
    change(&mut collapsed);
    write_atomic(
        &dir.join(COLLAPSED_FILE),
        &serde_json::to_vec(&collapsed).map_err(Error::unexpected)?,
    )?;
    Ok(())
}
/// Called under SETTINGS, before changing the configured folder. Unavailable folders leave legacy
/// bytes untouched and unread. Destination files win after a crash between write and cleanup.
fn try_migrate_derived(root: &Path, settings: &Path) {
    if migrate_derived(root, settings).is_err() {
        // Legacy restoration data must never lock the user out of ordinary notes/preferences.
        eprintln!("[pinned] derived-state migration deferred");
    }
}
/// Runs as every command opens the notes folder, so it costs one read and one look when there is
/// nothing left to move.
fn migrate_derived(root: &Path, settings: &Path) -> Result<(), Error> {
    let bytes = fs::read(settings).unwrap_or_default();
    let mut value = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
    let collapsed = value
        .as_ref()
        .and_then(|value| value.get("collapsed"))
        .cloned();
    let legacy = settings.with_file_name(PINNED_COPY_FILE);
    if collapsed.is_none() && !legacy.try_exists()? {
        return Ok(());
    }
    // Only into the folder chosen: a locked vault's empty mount point must stay empty.
    let chosen: Settings = serde_json::from_slice(&bytes).unwrap_or_default();
    let expected = chosen
        .folder_id
        .filter(|_| chosen.folder.as_deref() == Some(root));
    if expected.is_none() || read_marker(root).ok().flatten() != expected {
        return Ok(());
    }
    let _held = hold_ancestry(root)?;
    if legacy.try_exists()? {
        let copy =
            read_copy(&legacy).ok_or_else(|| Error::unexpected("invalid legacy pinned copy"))?;
        // A copy for an earlier folder stays until that folder is selected and available again.
        if copy.folder == root {
            let (dir, _held) = held_derived_dir(root)?;
            let dest = dir.join(PINNED_COPY_FILE);
            if !dest.try_exists()? {
                write_atomic(
                    &dest,
                    &serde_json::to_vec(&copy).map_err(Error::unexpected)?,
                )?;
            }
            read_copy(&dest).ok_or_else(|| Error::unexpected("invalid destination pinned copy"))?;
            fs::remove_file(&legacy)?;
        }
    }
    if let (Some(value), Some(old)) = (value.as_mut(), collapsed) {
        let (dir, _held) = held_derived_dir(root)?;
        let dest = dir.join(COLLAPSED_FILE);
        let targets: BTreeSet<String> = serde_json::from_value(old).map_err(Error::unexpected)?;
        if !dest.try_exists()? {
            write_atomic(
                &dest,
                &serde_json::to_vec(&targets).map_err(Error::unexpected)?,
            )?;
        }
        read_collapsed(&dir)?;
        value
            .as_object_mut()
            .ok_or_else(|| Error::unexpected("invalid settings"))?
            .remove("collapsed");
        write_atomic(
            settings,
            &serde_json::to_vec(value).map_err(Error::unexpected)?,
        )?;
    }
    Ok(())
}
fn read_copy(store: &Path) -> Option<PinnedCopy> {
    serde_json::from_slice(&fs::read(store).ok()?).ok()
}
/// The stored copy, if it holds anything worth restoring.
fn restorable_copy(store: &Path) -> Option<String> {
    read_copy(store)
        .filter(|copy| !copy.text.trim().is_empty())
        .map(|copy| copy.text)
}
/// Best effort: a copy that cannot be written only means a deleted file cannot be restored.
fn keep_copy(store: &Path, root: &Path, text: &str) {
    if read_copy(store).is_some_and(|copy| copy.text == text) {
        return;
    }
    let copy = PinnedCopy {
        folder: root.to_owned(),
        text: text.to_owned(),
    };
    let written = hold_ancestry(store.parent().unwrap_or(root))
        .and_then(|_held| write_atomic(store, &serde_json::to_vec(&copy)?));
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
    let _held = hold_ancestry(store.parent().ok_or(Error::InvalidPath)?)?;
    let Some((text, version)) = read_pinned(root)? else {
        return Ok(Pinned {
            version: String::new(),
            items: Vec::new(),
            restorable: restorable_copy(store).is_some(),
        });
    };
    keep_copy(store, root, &text);
    drop(guard);
    let canonical = root.canonicalize()?;
    let items = pinned::parse(&text)
        .into_iter()
        .map(|item| PinnedItem {
            missing: item.kind == Kind::Note
                && resolve_below(&canonical, &item.target, markdown).is_err(),
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
    let _held = hold_ancestry(store.parent().ok_or(Error::InvalidPath)?)?;
    if let Some(text) = restorable_copy(store) {
        save_note_in(root, PINNED, &text, "")?;
    }
    Ok(())
}
/// The pinned list of `root` as the UI shows it. Each pinned command opens the notes folder once and
/// answers this, so the UI needs no second call.
fn pinned_in(root: &Path) -> Result<Pinned, Error> {
    let (dir, _held) = held_derived_dir(root)?;
    pinned_state(root, &dir.join(PINNED_COPY_FILE), &read_collapsed(&dir)?)
}
pub fn pinned(app: &AppHandle) -> Result<Pinned, Error> {
    pinned_in(&root(app)?)
}
/// `target` is a note's path (it must exist) or an http(s) URL. `title` is the link text.
pub fn pin(app: &AppHandle, target: &str, title: &str) -> Result<Pinned, Error> {
    let root = root(app)?;
    pin_in(&root, target, title)?;
    pinned_in(&root)
}
fn pin_in(root: &Path, target: &str, title: &str) -> Result<(), Error> {
    let (kind, target) = match pinned_kind(target) {
        Kind::Url => {
            let url = tauri::Url::parse(target).map_err(|e| Error::InvalidUrl {
                detail: e.to_string(),
            })?;
            (Kind::Url, url.to_string())
        }
        Kind::Note => {
            resolve(root, target, markdown)?;
            (Kind::Note, target.to_owned())
        }
    };
    edit_pinned(root, |text| pinned::pin(text, kind, &target, title))
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
    let root = root(app)?;
    edit_pinned(&root, |text| pinned::unpin(text, target))?;
    update_collapsed(&root, |collapsed| {
        collapsed.remove(target);
    })?;
    pinned_in(&root)
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
    let root = root(app)?;
    move_pinned_in(&root, target, to, base)?;
    pinned_in(&root)
}
pub fn set_collapsed(app: &AppHandle, target: &str, collapsed: bool) -> Result<Pinned, Error> {
    let root = root(app)?;
    update_collapsed(&root, |targets| {
        if collapsed {
            targets.insert(target.to_owned());
        } else {
            targets.remove(target);
        }
    })?;
    pinned_in(&root)
}
pub fn restore_pinned(app: &AppHandle) -> Result<Pinned, Error> {
    let root = root(app)?;
    {
        let (dir, _held) = held_derived_dir(&root)?;
        restore_pinned_in(&root, &dir.join(PINNED_COPY_FILE))?;
    }
    pinned_in(&root)
}
/// Bound IPC writes and disk use for accidental large clipboard images (10 MiB).
pub const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;
pub const fn image_too_large() -> Error {
    Error::ImageTooLarge {
        limit_mb: (MAX_ATTACHMENT_BYTES / (1024 * 1024)) as u64,
    }
}
fn attachment_extension(bytes: &[u8]) -> Result<&'static str, Error> {
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(image_too_large());
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

fn attachment_path(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    is_image(path)
        && parent
            .iter()
            .any(|name| name.eq_ignore_ascii_case("attachments"))
        && !parent
            .iter()
            .any(|name| name.to_string_lossy().starts_with('.'))
}

/// This scan must fail if a note might have been missed. Unlike the display list, a skipped
/// directory cannot be taken as evidence that a file is unused. Hold each directory while
/// visiting it, so a link cannot be swapped in during the scan.
fn scan_notes(dir: &Path, visit: &mut impl FnMut(&Path) -> Result<(), Error>) -> Result<(), Error> {
    let _held = platform::hold_directory(dir)?;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_file() {
            visit(&entry.path())?;
        } else if kind.is_dir() && !entry.file_name().eq_ignore_ascii_case(DERIVED) {
            scan_notes(&entry.path(), visit)?;
        }
    }
    Ok(())
}

fn attachment_note(path: &Path) -> Result<String, Error> {
    let mut text = String::new();
    platform::open_plain_file(path)?.read_to_string(&mut text)?;
    Ok(text)
}

/// Finds available attachments and checks references against live and trashed notes, but those of
/// trash entry `except`. With a selection, only those exact paths are considered, even when another
/// name aliases a real file.
fn unused_images_in(
    root: &Path,
    selection: Option<&BTreeSet<&str>>,
    except: Option<&str>,
) -> Result<Vec<String>, Error> {
    let root = root.canonicalize()?;
    let _held = hold_ancestry(&root)?;
    let mut candidates = Vec::new();
    let mut notes = Vec::new();
    scan_notes(&root, &mut |file| {
        let path = file.strip_prefix(&root).map_err(|_| Error::OutsideFolder)?;
        if markdown(file) {
            notes.push((relative(&root, file), attachment_note(file)?));
        } else if attachment_path(path) {
            let path = relative(&root, file);
            if selection.is_none_or(|selected| selected.contains(path.as_str())) {
                candidates.push(path);
            }
        }
        Ok(())
    })?;
    if candidates.is_empty() {
        return Ok(candidates);
    }

    let dir = root.join(DERIVED).join(trash::DIR);
    match hold_ancestry(&dir) {
        Ok(_held) => scan_notes(&dir, &mut |file| {
            if markdown(file) {
                // Each entry holds what was deleted at its original path. For a deleted folder,
                // that includes every note below it.
                let path = file.strip_prefix(&dir).map_err(|_| Error::OutsideFolder)?;
                if except.is_some_and(|id| path.starts_with(id)) {
                    return Ok(());
                }
                let original: PathBuf = path.iter().skip(1).collect();
                let original = if original.as_os_str().is_empty() {
                    path
                } else {
                    &original
                };
                notes.push((relative(Path::new(""), original), attachment_note(file)?));
            }
            Ok(())
        })?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(trash_error(&dir, error)),
    }

    let used = attachments::referenced(
        &candidates,
        notes
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str())),
    );
    let mut unused: Vec<_> = candidates
        .iter()
        .filter(|p| !used.contains(p.as_str()))
        .cloned()
        .collect();
    sort_paths(&mut unused, |path| path);
    Ok(unused)
}

/// The images in `attachments` folders below the canonical `root`, which the caller holds, that the
/// note at `note` references (`attachments::referenced`).
fn images_of(root: &Path, note: &Path) -> Result<Vec<String>, Error> {
    let mut candidates = Vec::new();
    scan_notes(root, &mut |file| {
        let path = file.strip_prefix(root).map_err(|_| Error::OutsideFolder)?;
        if attachment_path(path) {
            candidates.push(relative(root, file));
        }
        Ok(())
    })?;
    let text = attachment_note(note)?;
    let path = relative(root, note);
    let mine = attachments::referenced(&candidates, [(path.as_str(), text.as_str())]);
    Ok(mine.into_iter().map(str::to_owned).collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnusedImages {
    folder_id: String,
    paths: Vec<String>,
}

pub fn unused_images(app: &AppHandle) -> Result<UnusedImages, Error> {
    let root = root(app)?;
    let folder_id = read_marker(&root)?.ok_or(Error::FolderUnrecognized)?;
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    Ok(UnusedImages {
        folder_id,
        paths: unused_images_in(&root, None, None)?,
    })
}

/// Sends away those of `paths` that no note references, live or in the trash but in entry `entry`:
/// into that entry of the notes folder's own trash, under `.images` (decision 0013), otherwise as
/// `send_away` does, but only once the entry is known to be gone: any other failure keeps the image
/// where it is. Only while `root`'s marker still holds `folder_id` (`ImagesFolderChanged`).
fn send_images_in(
    root: &Path,
    paths: &[String],
    entry: Option<&str>,
    (folder_id, chosen): (&str, u64),
    recycle: impl Fn(&Path) -> io::Result<Option<PathBuf>>,
) -> Result<(), Error> {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    if folder_choices() != chosen || read_marker(root)?.as_deref() != Some(folder_id) {
        return Err(Error::ImagesFolderChanged);
    }
    let root = root.canonicalize()?;
    let selection = paths.iter().map(String::as_str).collect();
    for path in unused_images_in(&root, Some(&selection), entry)? {
        let path = checked(&path, attachment_path)?;
        let parent = path.parent().ok_or(Error::InvalidPath)?;
        let (dir, _held) = hold_folder(&root, parent)?;
        let target = dir.join(path.file_name().ok_or(Error::InvalidPath)?);
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        if target.canonicalize()? != target {
            continue;
        }
        if let Some(id) = entry {
            let put = trash_dir(&root).and_then(|(trash, _held)| {
                trash::put_image(&trash, id, path, &target).map_err(Error::from)
            });
            let Err(error) = put else { continue };
            eprintln!("[delete] image not in its note's entry: {error:?}");
            let gone = trash_path(&root).map(|trash| fs::symlink_metadata(trash.join(id)));
            if !matches!(gone, Ok(Err(e)) if e.kind() == io::ErrorKind::NotFound) {
                continue;
            }
        }
        send_away(&root, &target, &recycle)?;
    }
    Ok(())
}

pub fn recycle_images(app: &AppHandle, folder_id: &str, paths: &[String]) -> Result<(), Error> {
    let chosen = folder_choices();
    send_images_in(
        &root(app)?,
        paths,
        None,
        (folder_id, chosen),
        platform::recycle,
    )
}

fn paste_image_in(root: &Path, path: &str, bytes: &[u8]) -> Result<String, Error> {
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
    let held = hold_ancestry(parent).map_err(|_| Error::AttachmentFolderUnsafe)?;
    // Revalidate after locking: resolve may have raced with an ancestor rename.
    if resolve(root, path, markdown)? != note {
        return Err(Error::AttachmentFolderUnsafe);
    }
    let image = attach(&note, held, bytes, ext)?;
    Ok(attachment_link(&image))
}
/// Writes an image named after `note` into its canonical parent's `attachments` folder, holding
/// the ancestry in `held`. Adds a counter when the name is taken. Answers the new file's path.
fn attach(note: &Path, mut held: Vec<fs::File>, bytes: &[u8], ext: &str) -> Result<PathBuf, Error> {
    let parent = note.parent().ok_or(Error::InvalidPath)?;
    let stem = note.file_stem().ok_or(Error::InvalidPath)?;
    let mut name = platform::fit_name(stem, platform::MAX_NAME - 1 - ext.len());
    name.push(format!(".{ext}"));
    let folder = parent.join("attachments");
    match fs::create_dir(&folder) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    held.push(platform::hold_directory(&folder).map_err(|_| Error::AttachmentFolderUnsafe)?);
    let temp = write_temp(&folder.join(&name), bytes)?;
    let numbered = (2..=u32::MAX).map(|n| format!("-{n}"));
    let placed = place(&temp, &folder, &name, false, numbered, |_| false);
    if placed.is_err() {
        let _ = fs::remove_file(&temp);
    }
    placed
}
/// The link from a note to an image `attach` put beside it.
fn attachment_link(image: &Path) -> String {
    const ENCODE: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    let name = image.file_name().unwrap_or_default().to_string_lossy();
    format!("attachments/{}", utf8_percent_encode(&name, ENCODE))
}
pub fn paste_image(app: &AppHandle, path: &str, bytes: &[u8]) -> Result<String, Error> {
    paste_image_in(&root(app)?, path, bytes)
}
/// Creates the note `path` holding `content` and then a link to `image`, which goes into the
/// attachments beside it as a pasted image does, in the folder whose marker holds `folder_id`, the
/// one the capture started in; another answers `FolderChanged` before anything is written. A taken
/// name answers `Conflict`, also before anything is written. Then the image is written and the note
/// created whole by `publish`, in one write and only while its name is still free: nothing writes
/// the note again, so a note deleted meanwhile cannot come back and the note never exists without its
/// link. Nothing is ever deleted: if the note is not created, the image stays and `ImageKept` names it.
fn create_with_image_in(
    root: &Path,
    folder_id: &str,
    path: &str,
    content: &str,
    image: &[u8],
    publish: fn(&Path, &str, &[u8]) -> Result<Saved, Error>,
) -> Result<Saved, Error> {
    let ext = attachment_extension(image)?;
    too_large(content.len() as u64)?;
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    if read_marker(root)?.as_deref() != Some(folder_id) {
        return Err(Error::FolderChanged);
    }
    // Reservations name the folder as configured, not canonical.
    if reserved(root, path) {
        return Ok(Saved::Conflict);
    }
    let target = root.canonicalize()?.join(checked(path, markdown)?);
    if exists(&target) {
        return Ok(Saved::Conflict);
    }
    let parent = target.parent().ok_or(Error::InvalidPath)?;
    // No link on the way: the image must land beside the note, inside the folder.
    if parent.canonicalize()? != parent {
        return Err(Error::AttachmentFolderUnsafe);
    }
    let held = hold_ancestry(parent).map_err(|_| Error::AttachmentFolderUnsafe)?;
    let attached = attach(&target, held, image, ext)?;
    let note = format!("{content}![]({})", attachment_link(&attached));
    match publish(root, path, note.as_bytes()) {
        Ok(saved @ Saved::Saved { .. }) => Ok(saved),
        // Another app took the name since the check, or the write failed.
        _ => Err(Error::ImageKept {
            path: display(&attached),
        }),
    }
}
/// The notes folder a capture goes into, as the path and marker id it has now: `create_with_image`
/// writes only there.
pub fn capture_folder(app: &AppHandle) -> Result<(String, String), Error> {
    let root = root(app)?;
    let id = read_marker(&root)?.ok_or(Error::FolderUnrecognized)?;
    Ok((root.to_string_lossy().into_owned(), id))
}
/// `create_with_image_in` in the notes folder, if it is still the one `capture_folder` answered
/// `folder` and `folder_id` for when the capture started.
pub fn create_with_image(
    app: &AppHandle,
    folder: &str,
    folder_id: &str,
    path: &str,
    content: &str,
    image: &[u8],
) -> Result<Saved, Error> {
    let root = root(app)?;
    if root.to_string_lossy() != folder {
        return Err(Error::FolderChanged);
    }
    create_with_image_in(&root, folder_id, path, content, image, recreate_note)
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
/// Milliseconds since the Unix epoch, if the time is known.
fn millis(time: io::Result<std::time::SystemTime>) -> Option<u64> {
    let since = time.ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(since.as_millis() as u64)
}
fn modified(path: &Path) -> u64 {
    millis(fs::metadata(path).and_then(|m| m.modified())).unwrap_or(0)
}
/// Taken under the save lock: a save in progress owns its backup for a moment, and that is not a
/// leftover.
fn leftovers_in(root: &Path) -> Result<Vec<Leftover>, Error> {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let mut found = Vec::new();
    walk(root, &mut |entry, folder| {
        if folder {
            return;
        }
        let path = entry.path();
        if let Some(note) = leftover_note(&path) {
            found.push(Leftover {
                path: relative(root, &path),
                note: relative(root, &note),
                modified: entry
                    .metadata()
                    .ok()
                    .and_then(|m| millis(m.modified()))
                    .unwrap_or(0),
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
/// `<stem> (<label> 2).md` and so on, as `place` does. Never overwrites, never deletes and never takes
/// a name a Recycle Bin Undo reserves. Returns the new note's path.
fn restore_in(root: &Path, leftover: &str, label: &str) -> Result<String, Error> {
    let _guard = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    let canonical = root.canonicalize()?;
    let source = resolve(&canonical, leftover, is_leftover)?;
    let note = leftover_note(&source).ok_or(Error::InvalidPath)?;
    let (folder, name) = (note.parent(), note.file_name());
    let (folder, name) = folder.zip(name).ok_or(Error::InvalidPath)?;
    let numbered = (2..=100).map(|n| match n {
        2 => format!(" ({label})"),
        _ => format!(" ({label} {})", n - 1),
    });
    let target = place(&source, folder, name, false, numbered, |candidate| {
        reserved(root, &relative(&canonical, candidate))
    })?;
    Ok(relative(&canonical, &target))
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
/// The `web` window also has the protocol, but its content is remote: only app windows read images.
/// `no-store` prevents the WebView2 profile shared between windows from caching the image.
fn image_response(webview: &str, root: Option<PathBuf>, uri_path: &str) -> Response<Vec<u8>> {
    if !crate::main_window::reads_images(webview) {
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
    image_response(webview, root(app).ok().map(|root| root.path), uri_path)
}
/// `path`, relative to the recognized notes folder, is an image `note-asset` would serve. Returns
/// the folder choices counted before the check, so the caller can tell if another folder was chosen
/// since.
pub fn check_image(app: &AppHandle, path: &str) -> Result<u64, Error> {
    let choices = folder_choices();
    resolve(&root(app)?, path, is_image)?;
    Ok(choices)
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
    /// Refuses listing a folder to everyone, by SID, until dropped; declared after `Temp`, so the
    /// folder can be deleted.
    struct Unlistable(PathBuf);
    impl Unlistable {
        fn new(dir: &Path) -> Self {
            assert!(icacls(dir, &["/deny", "*S-1-1-0:(RD)"]));
            Self(dir.to_owned())
        }
    }
    impl Drop for Unlistable {
        fn drop(&mut self) {
            icacls(&self.0, &["/remove:d", "*S-1-1-0"]);
        }
    }
    fn icacls(dir: &Path, args: &[&str]) -> bool {
        std::process::Command::new("icacls")
            .arg(dir)
            .args(args)
            .output()
            .is_ok_and(|out| out.status.success())
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
    fn a_field_that_does_not_parse_reads_as_its_default_and_keeps_the_others() {
        let defaults = serde_json::to_value(Settings {
            folder: Some("notes".into()),
            ..Settings::default()
        })
        .unwrap();
        let flags = ["null", "42", "{}", "[]", r#""true""#].as_slice();
        for (field, values) in [
            ("allowed_sites", ["42", "{}"].as_slice()),
            ("site_permissions", &["42", "{}"]),
            ("theme", &["null", "42", "{}", "true", r#""future-theme""#]),
            ("search_engine", &["null", "42", "{}", r#""future-engine""#]),
            ("custom_search_url", &["null", "42", "{}"]),
            ("note_order", &["null", "42", "{}", r#""future-order""#]),
            ("group_by_date", flags),
            ("relative_dates", flags),
            ("note_search_suggestions", flags),
            ("spelling_off", flags),
            ("group_by_folder", flags),
            ("vault_file", &["42", "{}", "[]"]),
            ("side", &[r#""top""#, "42"]),
            ("width", &[r#""wide""#, "-1", "{}"]),
            ("activation_shortcut", &["42", "{}", "[]"]),
            ("folder_id", &["42", "{}"]),
        ] {
            for value in values {
                let json = format!(r#"{{"folder":"notes","{field}":{value}}}"#);
                let settings: Settings = serde_json::from_str(&json).unwrap();
                assert_eq!(serde_json::to_value(settings).unwrap(), defaults, "{json}");
            }
        }
    }
    #[test]
    fn search_provider_settings_read_existing_and_new_choices() {
        use crate::web::SearchEngine;
        for (name, engine) in [
            ("duckduckgo", SearchEngine::DuckDuckGo),
            ("brave", SearchEngine::Brave),
            ("startpage", SearchEngine::Startpage),
            ("ecosia", SearchEngine::Ecosia),
            ("google", SearchEngine::Google),
            ("bing", SearchEngine::Bing),
            ("custom", SearchEngine::Custom),
        ] {
            let json = serde_json::json!({"search_engine": name});
            let settings: Settings = serde_json::from_value(json).unwrap();
            assert_eq!(settings.search_engine, engine);
            assert_eq!(
                serde_json::to_value(settings).unwrap()["search_engine"],
                name
            );
        }
    }
    #[test]
    fn site_exceptions_keep_valid_entries_and_drop_widened_ones() {
        let settings: Settings = serde_json::from_str(
            r#"{"allowed_sites":["example.com",5,"www.other.net","x.net#.example.com"]}"#,
        )
        .unwrap();
        assert_eq!(settings.allowed_sites, ["example.com"]);
    }

    // Catch arbitrary-file launches through an edited setting or a picker with the filter bypassed.
    #[test]
    fn vault_refuses_other_extensions() {
        let t = Temp::new();
        let file = t.0.join("vault.exe");
        fs::write(&file, "").unwrap();
        assert_eq!(validate_vault_file(&file), Err(Error::InvalidVaultFile));
    }
    #[test]
    fn vault_refuses_relative_paths() {
        assert_eq!(
            validate_vault_file(Path::new("vault.cryptomator")),
            Err(Error::InvalidVaultFile)
        );
    }
    #[test]
    fn vault_reports_removed_file() {
        let t = Temp::new();
        assert_eq!(
            validate_vault_file(&t.0.join("vault.cryptomator")),
            Err(Error::VaultFileMissing)
        );
    }
    #[test]
    fn vault_refuses_directory_with_matching_extension() {
        let t = Temp::new();
        let path = t.0.join("vault.cryptomator");
        fs::create_dir(&path).unwrap();
        assert_eq!(validate_vault_file(&path), Err(Error::InvalidVaultFile));
    }
    #[test]
    fn vault_accepts_existing_file_case_insensitively() {
        let t = Temp::new();
        let file = t.0.join("vault.CRYPTOMATOR");
        fs::write(&file, "").unwrap();
        assert_eq!(
            validate_vault_file(&file).unwrap(),
            fs::canonicalize(file).unwrap()
        );
    }
    #[test]
    fn vault_refuses_link_to_another_file_type() {
        let t = Temp::new();
        let file = t.0.join("vault.exe");
        let link = t.0.join("vault.cryptomator");
        fs::write(&file, "").unwrap();
        if !mklink(&[], &link, &file) {
            return;
        }
        assert_eq!(validate_vault_file(&link), Err(Error::InvalidVaultFile));
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
        let paths: Vec<_> = list(&t.0)
            .unwrap()
            .notes
            .into_iter()
            .map(|n| n.path)
            .collect();
        assert_eq!(paths, [".ponto.md", "A.md", "sub/b.md", "z.MD"]);
    }
    #[test]
    fn the_watcher_reports_changes_to_what_is_listed_only() {
        let root = Path::new(r"C:\notes");
        let shown = |p: &str| shown_change(root, &root.join(p));
        assert!(shown(".ponto.md") && shown("sub/b.md") && shown("sub"));
        assert!(!shown(".oculta/segredo.md") && !shown("a.md.1-2.docdock-tmp"));
    }
    #[test]
    fn a_subfolder_that_cannot_be_read_is_skipped_and_reported() {
        let t = Temp::new();
        for dir in ["locked/inner", "sub"] {
            fs::create_dir_all(t.0.join(dir)).unwrap();
        }
        for note in ["a.md", "locked/b.md", "sub/c.md"] {
            fs::write(t.0.join(note), "").unwrap();
        }
        let _refused = Unlistable::new(&t.0.join("locked"));
        let listing = list(&t.0).unwrap();
        let paths: Vec<_> = listing.notes.into_iter().map(|n| n.path).collect();
        assert_eq!(paths, ["a.md", "sub/c.md"]);
        assert_eq!(listing.skipped, ["locked"]);
        assert_eq!(folders_in(&t.0).unwrap(), ["sub"]);
    }
    #[test]
    fn a_notes_folder_that_cannot_be_read_fails_the_listing() {
        let t = Temp::new();
        let notes = t.0.join("notes");
        fs::create_dir(&notes).unwrap();
        let _refused = Unlistable::new(&notes);
        assert!(list(&notes).is_err());
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
    // Internal data must never become editable notes through a hand-written pin or alias.
    fn internal_notes(t: &Temp) -> Vec<String> {
        [".docdock", ".docdock/trash/x", "nested/.DOCDOCK"]
            .into_iter()
            .map(|dir| {
                fs::create_dir_all(t.0.join(dir)).unwrap();
                let path = format!("{dir}/x.md");
                fs::write(t.0.join(&path), "private").unwrap();
                path
            })
            .collect()
    }
    #[test]
    fn opening_internal_notes_is_refused() {
        let t = Temp::new();
        for path in internal_notes(&t) {
            assert_eq!(resolve(&t.0, &path, markdown), Err(Error::InvalidPath));
        }
    }
    #[test]
    fn saving_internal_notes_is_refused() {
        let t = Temp::new();
        for path in internal_notes(&t) {
            assert_eq!(
                save_note_in(&t.0, &path, "changed", &version(b"private")),
                Err(Error::InvalidPath)
            );
            assert_eq!(fs::read_to_string(t.0.join(path)).unwrap(), "private");
        }
    }
    #[test]
    fn recreating_internal_notes_is_refused() {
        let t = Temp::new();
        for path in internal_notes(&t) {
            fs::remove_file(t.0.join(&path)).unwrap();
            assert_eq!(
                save_note_in(&t.0, &path, "changed", ""),
                Err(Error::InvalidPath)
            );
            assert!(!t.0.join(path).exists());
        }
    }
    #[test]
    fn pinning_internal_notes_is_refused() {
        let t = Temp::new();
        for path in internal_notes(&t) {
            assert_eq!(pin_in(&t.0, &path, "private"), Err(Error::InvalidPath));
        }
        assert!(!t.0.join(PINNED).exists());
    }
    #[test]
    fn canonical_aliases_cannot_open_internal_notes() {
        let t = Temp::new();
        for (index, path) in internal_notes(&t).iter().enumerate() {
            let alias = format!("alias{index}");
            assert!(
                mklink(
                    &["/J"],
                    &t.0.join(&alias),
                    &t.0.join(path).parent().unwrap().canonicalize().unwrap()
                ),
                "could not create alias {index} for {path}"
            );
            assert_eq!(
                resolve(&t.0, &format!("{alias}/x.md"), markdown),
                Err(Error::InvalidPath)
            );
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
        assert!(list(&notas).unwrap().notes.is_empty());
        assert!(resolve(&notas, "atalho/segredo.md", markdown).is_err());
    }
    #[test]
    fn pasted_images_accept_only_known_formats() {
        let t = Temp::new();
        fs::create_dir(t.0.join("sub")).unwrap();
        fs::write(t.0.join("sub/a.md"), "unchanged").unwrap();
        let png = b"\x89PNG\r\n\x1a\n";
        let paste = |bytes: &[u8]| paste_image_in(&t.0, "sub/a.md", bytes);
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
        assert!(paste_image_in(&t.0, "../a.md", png).is_err());
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
        let paste = |bytes: &[u8]| paste_image_in(&t.0, "sub/a.md", bytes);
        let first = paste(png).unwrap();
        assert_eq!(first, "attachments/a.png");
        let folder = t.0.join("sub/attachments");
        fs::write(folder.join("A-2.PNG"), "kept").unwrap();
        let next = paste(b"\x89PNG\r\n\x1a\nnext").unwrap();
        assert_eq!(next, "attachments/a-3.png");
        assert_eq!(fs::read_to_string(folder.join("A-2.PNG")).unwrap(), "kept");
        let response = image_response("main", Some(t.0.clone()), "/sub/attachments/a.png");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body(), png);
        assert_eq!(
            fs::read_to_string(t.0.join("sub/a.md")).unwrap(),
            "unchanged"
        );
    }
    /// A notes folder marked as chosen, for captures.
    fn marked() -> (Temp, String) {
        let t = Temp::new();
        let id = stamp(&t.0).unwrap();
        (t, id)
    }
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nimage";
    #[test]
    fn a_captured_note_is_created_with_its_image_linked() {
        let (t, id) = marked();
        let created = create_with_image_in(
            &t.0,
            &id,
            "Page (%20) & fragment #1.md",
            "---\n---\n\n",
            PNG,
            recreate_note,
        );
        assert!(matches!(created, Ok(Saved::Saved { .. })));
        assert_eq!(
            fs::read_to_string(t.0.join("Page (%20) & fragment #1.md")).unwrap(),
            "---\n---\n\n![](attachments/Page%20%28%2520%29%20%26%20fragment%20%231.png)"
        );
        assert_eq!(
            fs::read(t.0.join("attachments/Page (%20) & fragment #1.png")).unwrap(),
            PNG
        );
    }
    #[test]
    fn a_capture_under_a_taken_name_conflicts_before_writing_its_image() {
        let (t, id) = marked();
        fs::write(t.0.join("Page.md"), "kept").unwrap();
        assert_eq!(
            create_with_image_in(&t.0, &id, "Page.md", "new", PNG, recreate_note),
            Ok(Saved::Conflict)
        );
        assert_eq!(fs::read_to_string(t.0.join("Page.md")).unwrap(), "kept");
        assert!(!t.0.join("attachments").exists());
    }
    #[test]
    fn a_capture_whose_note_fails_keeps_its_image() {
        let (t, id) = marked();
        let failed = create_with_image_in(&t.0, &id, "Page.md", "", PNG, |_, _, _| {
            Err(Error::unexpected("disk full"))
        });
        let image = t.0.canonicalize().unwrap().join("attachments/Page.png");
        assert_eq!(
            failed,
            Err(Error::ImageKept {
                path: display(&image)
            })
        );
        assert_eq!(fs::read(&image).unwrap(), PNG);
    }
    #[test]
    fn a_capture_into_another_folder_at_the_same_path_writes_nothing() {
        let (t, _) = marked();
        let started = new_folder_id();
        assert_eq!(
            create_with_image_in(&t.0, &started, "Page.md", "", PNG, recreate_note),
            Err(Error::FolderChanged)
        );
        assert!(!t.0.join("Page.md").exists());
        assert!(!t.0.join("attachments").exists());
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
            paste_image_in(&t.0.join("notes"), "a.md", b"GIF89a"),
            Err(Error::AttachmentFolderUnsafe)
        );
        assert_eq!(fs::read_dir(t.0.join("outside")).unwrap().count(), 0);
    }
    #[test]
    fn images_reach_only_app_windows() {
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

        let preview = pedir("image-preview", "/com%20espaco/foto.PNG");
        assert_eq!(preview.status(), StatusCode::OK);
        for label in ["web", "tweak", "float-9"] {
            let refused = pedir(label, "/com%20espaco/foto.PNG");
            assert_eq!(refused.status(), StatusCode::FORBIDDEN, "{label}");
            assert!(refused.body().is_empty(), "{label}");
        }

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
        let paths: Vec<_> = list(&t.0)
            .unwrap()
            .notes
            .into_iter()
            .map(|n| n.path)
            .collect();
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
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
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
        let store = held_derived_dir(&root).unwrap().0.join(PINNED_COPY_FILE);
        (root, store)
    }
    fn no_collapsed() -> BTreeSet<String> {
        BTreeSet::new()
    }
    /// `settings.json` in `t` after choosing `root`, with `extra` fields added.
    fn chosen(t: &Temp, root: &Path, extra: serde_json::Value) -> PathBuf {
        let settings = t.0.join("settings.json");
        choose_in(&settings, root.to_owned()).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
        for (key, field) in extra.as_object().unwrap() {
            value[key] = field.clone();
        }
        fs::write(&settings, serde_json::to_vec(&value).unwrap()).unwrap();
        settings
    }
    fn load_value(settings: &Path) -> serde_json::Value {
        serde_json::from_slice(&fs::read(settings).unwrap()).unwrap()
    }
    /// `settings.json` in `t` as written before markers existed: `fields` without an id.
    fn earlier_settings(t: &Temp, fields: serde_json::Value) -> PathBuf {
        let settings = t.0.join("settings.json");
        fs::write(&settings, serde_json::to_vec(&fields).unwrap()).unwrap();
        settings
    }
    /// Every path below `dir`, relative to it.
    fn tree(dir: &Path) -> Vec<String> {
        let mut found = Vec::new();
        fn visit(root: &Path, dir: &Path, found: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                found.push(relative(root, &path));
                if path.is_dir() {
                    visit(root, &path, found);
                }
            }
        }
        visit(dir, dir, &mut found);
        found.sort();
        found
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
            delete_folder_in(&t.0, "parent/empty", |_| panic!(
                "empty folder must not recycle"
            )),
            Ok(Removal::default())
        );
        assert!(!t.0.join("parent/empty").exists());
        assert!(t.0.join("parent").is_dir());
    }
    #[test]
    fn deleting_a_folder_takes_hidden_content_to_the_recycle_bin() {
        let t = Temp::new();
        fs::create_dir(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/.hidden"), "keep").unwrap();
        let bin = t.0.join("bin");
        assert_eq!(
            delete_folder_in(&t.0, "folder", |p| to_bin(p, &bin)),
            Ok(Removal::default())
        );
        assert_eq!(
            fs::read_to_string(bin.join("folder/.hidden")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn deleting_a_folder_removes_only_pins_inside_it() {
        let t = Temp::new();
        fs::create_dir_all(t.0.join("folder/sub")).unwrap();
        fs::write(t.0.join("folder/sub/a.md"), "keep in bin").unwrap();
        fs::write(t.0.join(PINNED), "# Pins\n- [A](FOLDER/sub/a.md)\n- [B](folder-other/a.md)\n- [Web](https://example.com/folder/a.md)\n").unwrap();
        let bin = t.0.join("bin");
        assert_eq!(
            delete_folder_in(&t.0, "folder", |p| to_bin(p, &bin)),
            Ok(Removal::default())
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
        delete_folder_in(&t.0, "folder", |p| to_bin(p, &bin)).unwrap();
        assert!(save_note_in(&t.0, "folder/a.md", "late save", &base).is_err());
        assert!(!t.0.join("folder").exists());
    }
    #[test]
    fn a_deleted_folder_can_be_explicitly_recreated_and_used() {
        let t = Temp::new();
        fs::create_dir(t.0.join("folder")).unwrap();
        delete_folder_in(&t.0, "folder", |_| unreachable!()).unwrap();
        create_folder_in(&t.0, "folder").unwrap();
        assert!(matches!(
            save_note_in(&t.0, "folder/a.md", "new", ""),
            Ok(Saved::Saved { .. })
        ));
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
                delete_folder_in(&t.0, path, |_| panic!("unsafe path")).is_err(),
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
            delete_folder_in(&t.0, &alias, |_| panic!("alias must not recycle")),
            Err(Error::InvalidPath)
        );
        assert!(target.is_dir());
    }
    #[test]
    fn deleting_a_folder_refuses_junctions_at_every_level() {
        let t = Temp::new();
        fs::create_dir_all(t.0.join("real/sub")).unwrap();
        assert!(mklink(&["/J"], &t.0.join("link"), &t.0.join("real")));
        for path in ["link", "link/sub"] {
            assert_eq!(
                delete_folder_in(&t.0, path, |_| panic!("junction")),
                Err(Error::OutsideFolder)
            );
        }
        assert!(t.0.join("real/sub").is_dir());
    }

    /// As `delete_note_in` where the Recycle Bin would take the note, if `recycle` lets it.
    fn delete_note_bin(
        root: &Path,
        relative: &str,
        recycle: impl Fn(&Path) -> io::Result<Option<PathBuf>>,
    ) -> Result<Removal, Error> {
        delete_note_in(root, relative, recycle, |_| Ok(true))
    }
    /// Moves a deleted note into `bin`, as the Recycle Bin would.
    /// Moves `file` into `bin`, as the Recycle Bin would, and answers where it landed.
    fn to_bin(file: &Path, bin: &Path) -> io::Result<Option<PathBuf>> {
        fs::create_dir_all(bin)?;
        let landed = bin.join(file.file_name().unwrap());
        fs::rename(file, &landed)?;
        Ok(Some(landed))
    }
    /// Whether Undo can bring the note back.
    fn delete(root: &Path, bin: &Path, path: &str) -> Result<bool, Error> {
        delete_note_bin(root, path, |file| to_bin(file, bin)).map(|removal| removal.undo)
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
        let deleted = delete_note_bin(&t.0, "a.md", |file| {
            // Another app moves the note away and puts a different file at its path.
            fs::rename(file, t.0.join("moved.md"))?;
            fs::write(file, "replacement")?;
            to_bin(file, &t.0.join("bin"))
        });
        assert_eq!(deleted.map(|removal| removal.undo), Ok(true));
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
        let deleted = delete_note_bin(&t.0, "a.md", |file| {
            forget_deleted();
            to_bin(file, &t.0.join("bin"))
        });
        assert_eq!(deleted.map(|removal| removal.undo), Ok(false));
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
    /// The Recycle Bin refusing, as on a vault: the item goes to the notes folder's own trash.
    fn refused_bin(_: &Path) -> io::Result<Option<PathBuf>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn trash_of(root: &Path) -> PathBuf {
        root.join(DERIVED).join(trash::DIR)
    }
    /// Deletes the note at `path` to the trash and answers its entry id.
    fn trash_note(root: &Path, path: &str) -> String {
        delete_note_bin(root, path, refused_bin)
            .unwrap()
            .trash
            .unwrap()
    }
    #[test]
    fn unused_candidates_are_supported_images_inside_visible_attachment_folders() {
        let t = Temp::new();
        let candidates = [
            "sub/ATTACHMENTS/deeper/cover.JPEG",
            "other/cover.png",
            ".hidden/attachments/hidden.png",
            "sub/ATTACHMENTS/.hidden/hidden.png",
            "sub/ATTACHMENTS/vector.svg",
        ];
        for path in candidates {
            let file = t.0.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, b"fixture").unwrap();
        }
        assert_eq!(unused_images_in(&t.0, None, None).unwrap(), [candidates[0]]);
    }
    #[test]
    fn a_note_in_a_trashed_folder_resolves_links_from_its_original_path() {
        let _serial = deleting();
        let t = Temp::new();
        fs::create_dir_all(t.0.join("sub/attachments")).unwrap();
        fs::create_dir_all(t.0.join("sub/folder")).unwrap();
        fs::write(t.0.join("sub/attachments/a&b.png"), "fixture").unwrap();
        fs::write(
            t.0.join("sub/folder/note.md"),
            "[Map](../attachments/a&amp;b.png)",
        )
        .unwrap();
        delete_folder_in(&t.0, "sub/folder", refused_bin).unwrap();
        assert!(unused_images_in(&t.0, None, None).unwrap().is_empty());
    }
    #[test]
    fn recycling_skips_an_attachment_referenced_since_it_was_listed() {
        let t = Temp::new();
        let path = "attachments/new.png";
        fs::create_dir(t.0.join("attachments")).unwrap();
        fs::write(t.0.join(path), b"fixture").unwrap();
        let selected = unused_images_in(&t.0, None, None).unwrap();
        assert_eq!(selected, [path]);
        fs::write(t.0.join("note.md"), "Look at NEW.PNG.").unwrap();
        let id = stamp(&t.0).unwrap();
        send_images_in(&t.0, &selected, None, (&id, folder_choices()), |_| {
            panic!("image is in use")
        })
        .unwrap();
        assert!(exists(&t.0.join(path)));
    }
    #[test]
    fn recycling_ignores_paths_outside_attachment_folders() {
        let t = Temp::new();
        fs::create_dir(t.0.join("attachments")).unwrap();
        fs::write(t.0.join("other.png"), b"fixture").unwrap();
        let selected = ["other.png".into(), "attachments/../other.png".into()];
        let id = stamp(&t.0).unwrap();
        send_images_in(&t.0, &selected, None, (&id, folder_choices()), |_| {
            panic!("image is outside attachments")
        })
        .unwrap();
        assert!(exists(&t.0.join("other.png")));
    }
    #[test]
    fn an_image_sent_to_the_folder_trash_restores_to_its_attachment_folder() {
        let t = Temp::new();
        let path = "topic/Attachments/maps/chart.webp";
        let image = t.0.join(path);
        fs::create_dir_all(image.parent().unwrap()).unwrap();
        fs::write(&image, b"image bytes").unwrap();
        let id = stamp(&t.0).unwrap();
        send_images_in(
            &t.0,
            &[path.into()],
            None,
            (&id, folder_choices()),
            refused_bin,
        )
        .unwrap();
        assert!(!exists(&image));
        let [item] = &trash_items_in(&t.0).unwrap()[..] else {
            panic!("one image should be in the trash");
        };
        assert_eq!(restore_trashed_in(&t.0, &item.id, false), Ok(path.into()));
        assert_eq!(fs::read(image).unwrap(), b"image bytes");
    }
    #[test]
    fn a_note_the_recycle_bin_refuses_goes_to_the_trash_inside_the_folder() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "secret").unwrap();
        fs::write(t.0.join("b.md"), "b").unwrap();
        let removal = delete_note_bin(&t.0, "a.md", refused_bin).unwrap();
        assert!(!removal.undo);
        let id = removal.trash.unwrap();
        assert!(!exists(&t.0.join("a.md")));
        assert_eq!(
            fs::read_to_string(trash_of(&t.0).join(&id).join("a.md")).unwrap(),
            "secret"
        );
        let listed: Vec<_> = list(&t.0)
            .unwrap()
            .notes
            .into_iter()
            .map(|n| n.path)
            .collect();
        assert_eq!(listed, ["b.md"]);
    }
    #[test]
    fn the_trash_lists_each_items_path_kind_and_date_newest_first() {
        let _serial = deleting();
        let t = Temp::new();
        fs::create_dir_all(t.0.join("sub/folder")).unwrap();
        fs::write(t.0.join("sub/a.md"), "a").unwrap();
        fs::write(t.0.join("sub/folder/b.md"), "b").unwrap();
        let before = trash::now_ms();
        let note = trash_note(&t.0, "sub/a.md");
        let folder = delete_folder_in(&t.0, "sub/folder", refused_bin)
            .unwrap()
            .trash
            .unwrap();
        let after = trash::now_ms();
        let items = trash_items_in(&t.0).unwrap();
        let shape: Vec<_> = items
            .iter()
            .map(|i| (i.id.as_str(), i.path.as_deref(), i.folder))
            .collect();
        assert_eq!(
            shape,
            [
                (folder.as_str(), Some("sub/folder"), true),
                (note.as_str(), Some("sub/a.md"), false)
            ]
        );
        assert!(items.iter().all(|i| (before..=after).contains(&i.deleted)));
    }
    #[test]
    fn undo_brings_a_trashed_note_and_its_pinned_items_back_after_the_panel_hides() {
        let _serial = deleting();
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        fs::write(root.join("a.md"), "body").unwrap();
        let list = "- [A](a.md)\n- [C](c.md)\n";
        fs::write(root.join(PINNED), list).unwrap();
        let id = trash_note(&root, "a.md");
        assert_eq!(
            fs::read_to_string(root.join(PINNED)).unwrap(),
            "- [C](c.md)\n"
        );
        forget_deleted();
        assert_eq!(restore_trashed_in(&root, &id, true), Ok("a.md".into()));
        assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "body");
        assert_eq!(fs::read_to_string(root.join(PINNED)).unwrap(), list);
        assert!(!exists(&trash_of(&root).join(&id)));
        assert_eq!(
            restore_trashed_in(&root, &id, true),
            Err(Error::TrashItemMissing)
        );
    }
    #[test]
    fn an_entry_that_fails_to_empty_keeps_its_pinned_items_for_undo() {
        let _serial = deleting();
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        fs::write(root.join("a.md"), "body").unwrap();
        let list = "- [A](a.md)\n";
        fs::write(root.join(PINNED), list).unwrap();
        let id = trash_note(&root, "a.md");
        // An open handle without delete sharing makes the removal fail.
        let open =
            std::os::windows::fs::OpenOptionsExt::share_mode(fs::OpenOptions::new().read(true), 0)
                .open(trash_of(&root).join(&id).join("a.md"))
                .unwrap();
        assert!(empty_trash_in(&root, std::slice::from_ref(&id)).is_err());
        drop(open);
        assert_eq!(restore_trashed_in(&root, &id, true), Ok("a.md".into()));
        assert_eq!(fs::read_to_string(root.join(PINNED)).unwrap(), list);
    }
    #[test]
    fn undo_brings_a_trashed_folder_back_with_its_content_and_pins() {
        let _serial = deleting();
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        fs::create_dir_all(root.join("folder/attachments")).unwrap();
        fs::write(root.join("folder/a.md"), "a").unwrap();
        fs::write(root.join("folder/attachments/image.png"), "png").unwrap();
        let list = "- [A](folder/a.md)\n- [C](c.md)\n";
        fs::write(root.join(PINNED), list).unwrap();
        let id = delete_folder_in(&root, "folder", refused_bin)
            .unwrap()
            .trash
            .unwrap();
        assert!(!exists(&root.join("folder")));
        assert_eq!(restore_trashed_in(&root, &id, true), Ok("folder".into()));
        assert_eq!(fs::read_to_string(root.join("folder/a.md")).unwrap(), "a");
        assert!(exists(&root.join("folder/attachments/image.png")));
        assert_eq!(fs::read_to_string(root.join(PINNED)).unwrap(), list);
    }
    #[test]
    fn restoring_recreates_the_folders_gone_from_above_the_item() {
        let _serial = deleting();
        let t = Temp::new();
        fs::create_dir_all(t.0.join("sub/deep")).unwrap();
        fs::write(t.0.join("sub/deep/a.md"), "a").unwrap();
        let id = trash_note(&t.0, "sub/deep/a.md");
        fs::remove_dir_all(t.0.join("sub")).unwrap();
        assert_eq!(
            restore_trashed_in(&t.0, &id, false),
            Ok("sub/deep/a.md".into())
        );
        assert_eq!(fs::read_to_string(t.0.join("sub/deep/a.md")).unwrap(), "a");
    }
    #[test]
    fn restoring_takes_a_free_name_rather_than_overwrite() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "old").unwrap();
        let note = trash_note(&t.0, "a.md");
        fs::write(t.0.join("a.md"), "new").unwrap();
        assert_eq!(
            restore_trashed_in(&t.0, &note, false),
            Ok("a (2).md".into())
        );
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "new");
        assert_eq!(fs::read_to_string(t.0.join("a (2).md")).unwrap(), "old");
        fs::create_dir(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/a.md"), "old").unwrap();
        let folder = delete_folder_in(&t.0, "folder", refused_bin)
            .unwrap()
            .trash
            .unwrap();
        fs::create_dir(t.0.join("folder")).unwrap();
        assert_eq!(
            restore_trashed_in(&t.0, &folder, false),
            Ok("folder (2)".into())
        );
        assert_eq!(
            fs::read_to_string(t.0.join("folder (2)/a.md")).unwrap(),
            "old"
        );
    }
    #[test]
    fn emptying_removes_every_entry_and_never_follows_a_link() {
        let _serial = deleting();
        let t = Temp::new();
        let (root, outside) = (t.0.join("notes"), t.0.join("outside"));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("other.md"), "keep").unwrap();
        fs::create_dir_all(root.join("folder")).unwrap();
        fs::write(root.join("folder/a.md"), "a").unwrap();
        let id = delete_folder_in(&root, "folder", refused_bin)
            .unwrap()
            .trash
            .unwrap();
        let entry = trash_of(&root).join(&id).join("folder");
        assert!(mklink(&["/J"], &entry.join("link"), &outside));
        let ids: Vec<_> = trash_items_in(&root)
            .unwrap()
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(ids.len(), 1);
        assert_eq!(empty_trash_in(&root, &ids), Ok(()));
        assert!(!exists(&trash_of(&root)));
        assert_eq!(
            fs::read_to_string(outside.join("other.md")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn a_trash_that_is_a_link_is_refused() {
        let _serial = deleting();
        let t = Temp::new();
        let (root, outside) = (t.0.join("notes"), t.0.join("outside"));
        fs::create_dir_all(root.join(DERIVED)).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("other.md"), "keep").unwrap();
        assert!(mklink(&["/J"], &trash_of(&root), &outside));
        fs::write(root.join("a.md"), "a").unwrap();
        let deleted = delete_note_bin(&root, "a.md", refused_bin);
        assert!(
            matches!(deleted, Err(Error::DeleteFailed { .. })),
            "{deleted:?}"
        );
        assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "a");
        assert_eq!(trash_items_in(&root), Err(Error::OutsideFolder));
        assert!(empty_trash_in(&root, &["x".into()]).is_err());
        trash::purge(&trash_of(&root), u64::MAX);
        assert_eq!(
            fs::read_to_string(outside.join("other.md")).unwrap(),
            "keep"
        );
        assert!(
            fs::symlink_metadata(trash_of(&root))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
    #[test]
    fn purging_removes_only_entries_older_than_thirty_days() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "recent").unwrap();
        let recent = trash_note(&t.0, "a.md");
        // Deleted at 1 ms after the Unix epoch, by a computer whose clock ran late: it arrived here
        // just now, so it stays until it has also been here 30 days.
        let late = trash_of(&t.0).join("1-0-0-0");
        fs::create_dir_all(&late).unwrap();
        fs::write(late.join("late.md"), "late").unwrap();
        let old = trash_of(&t.0).join("1-1-0-0");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("old.md"), "old").unwrap();
        age(&old, trash::KEEP_MS + 60_000);
        fs::write(trash_of(&t.0).join("dropped here.md"), "not ours").unwrap();
        trash::purge(&trash_of(&t.0), trash::now_ms());
        assert!(!exists(&old));
        assert!(exists(&late.join("late.md")));
        assert!(exists(&trash_of(&t.0).join(&recent).join("a.md")));
        assert!(exists(&trash_of(&t.0).join("dropped here.md")));
    }
    /// Sets `dir`'s last change `ms` in the past.
    fn age(dir: &Path, ms: u64) {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_WRITE_ATTRIBUTES, and FILE_FLAG_BACKUP_SEMANTICS to open a folder.
        let handle = fs::OpenOptions::new()
            .access_mode(0x100)
            .custom_flags(0x0200_0000)
            .open(dir)
            .unwrap();
        let when = SystemTime::now() - std::time::Duration::from_millis(ms);
        handle.set_modified(when).unwrap();
    }
    /// Puts `content` in a hand-made trash entry of `root` at `path`, through a verbatim path so that
    /// names Windows would otherwise change, such as `con.md` or `folder.`, are made as written.
    fn plant(root: &Path, id: &str, path: &str, content: &str) {
        fs::create_dir_all(trash_of(root)).unwrap();
        let file = trash_of(&root.canonicalize().unwrap()).join(id).join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, content).unwrap();
    }
    #[test]
    fn restoring_refuses_internal_unsafe_and_device_names_from_a_hand_made_entry() {
        let _serial = deleting();
        let t = Temp::new();
        for (n, path) in [
            (0, "_pinned.md"),
            (1, ".docdock/x.md"),
            (2, "image.png"),
            (3, "con.md"),
            (4, "nul.txt.md"),
        ] {
            let depth = path.matches('/').count();
            let id = format!("{n}-0-0-{depth}");
            plant(&t.0, &id, path, "planted");
            assert_eq!(
                restore_trashed_in(&t.0, &id, false),
                Err(Error::InvalidPath),
                "{path}"
            );
        }
        plant(&t.0, "5-0-0-0", "folder./a.md", "planted");
        assert_eq!(
            restore_trashed_in(&t.0, "5-0-0-0", false),
            Err(Error::InvalidPath)
        );
        assert!(!exists(&t.0.join(PINNED)));
        assert!(!exists(&t.0.join("image.png")));
        // Names Windows would change can only go through a verbatim path.
        fs::remove_dir_all(trash_of(&t.0.canonicalize().unwrap())).unwrap();
    }
    #[test]
    fn a_link_in_an_entry_is_neither_listed_as_an_item_nor_restored() {
        let _serial = deleting();
        let t = Temp::new();
        let (root, outside) = (t.0.join("notes"), t.0.join("outside"));
        fs::create_dir_all(outside.join("inner")).unwrap();
        fs::write(outside.join("inner/secret.md"), "outside").unwrap();
        // A link in the chain of folders, then a link as the item itself.
        fs::create_dir_all(trash_of(&root).join("1-0-0-1")).unwrap();
        assert!(mklink(
            &["/J"],
            &trash_of(&root).join("1-0-0-1").join("sub"),
            &outside
        ));
        fs::create_dir_all(trash_of(&root).join("2-0-0-0")).unwrap();
        assert!(mklink(
            &["/J"],
            &trash_of(&root).join("2-0-0-0").join("item"),
            &outside
        ));
        let items = trash_items_in(&root).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.path.is_none()), "{items:?}");
        for id in ["1-0-0-1", "2-0-0-0"] {
            assert_eq!(
                restore_trashed_in(&root, id, false),
                Err(Error::InvalidPath)
            );
        }
        assert!(!exists(&root.join("sub")) && !exists(&root.join("item")));
        assert!(exists(&outside.join("inner/secret.md")));
    }
    #[test]
    fn shell_noise_beside_an_item_does_not_hide_it() {
        let _serial = deleting();
        let t = Temp::new();
        plant(&t.0, "1-0-0-1", "sub/a.md", "a");
        fs::write(trash_of(&t.0).join("1-0-0-1/desktop.ini"), "").unwrap();
        fs::write(trash_of(&t.0).join("1-0-0-1/sub/Thumbs.db"), "").unwrap();
        let items = trash_items_in(&t.0).unwrap();
        assert_eq!(items[0].path.as_deref(), Some("sub/a.md"));
        assert_eq!(
            restore_trashed_in(&t.0, "1-0-0-1", false),
            Ok("sub/a.md".into())
        );
        assert!(!exists(&trash_of(&t.0).join("1-0-0-1")));
    }
    #[test]
    fn two_items_in_one_entry_are_listed_as_unrecognized_and_emptied_only_when_listed() {
        let _serial = deleting();
        let t = Temp::new();
        plant(&t.0, "1-0-0-0", "a.md", "a");
        plant(&t.0, "1-0-0-0", "a (conflict).md", "b");
        let items = trash_items_in(&t.0).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path, None);
        // Synced in after the list was shown: Empty leaves it.
        plant(&t.0, "2-0-0-0", "late.md", "late");
        assert_eq!(empty_trash_in(&t.0, &["1-0-0-0".into()]), Ok(()));
        assert!(!exists(&trash_of(&t.0).join("1-0-0-0")));
        assert!(exists(&trash_of(&t.0).join("2-0-0-0/late.md")));
    }
    #[test]
    fn restoring_keeps_an_existing_parent_whose_name_the_app_would_not_give() {
        let _serial = deleting();
        let t = Temp::new();
        let parent = format!(" {}", "p".repeat(120));
        fs::create_dir(t.0.join(&parent)).unwrap();
        fs::write(t.0.join(&parent).join("a.md"), "a").unwrap();
        let id = trash_note(&t.0, &format!("{parent}/a.md"));
        assert_eq!(
            restore_trashed_in(&t.0, &id, false),
            Ok(format!("{parent}/a.md"))
        );
        // A missing one is still created only under the rules.
        let id = trash_note(&t.0, &format!("{parent}/a.md"));
        fs::remove_dir_all(t.0.join(&parent)).unwrap();
        assert_eq!(
            restore_trashed_in(&t.0, &id, false),
            Err(Error::InvalidFolderName)
        );
    }
    #[test]
    fn folders_made_for_a_restore_stay_held_until_released() {
        let t = Temp::new();
        let root = t.0.canonicalize().unwrap();
        let held = ensure_folders(&root, Path::new("a/b")).unwrap();
        assert!(fs::rename(root.join("a/b"), root.join("moved")).is_err());
        drop(held);
        assert!(fs::rename(root.join("a/b"), root.join("moved")).is_ok());
    }
    #[test]
    fn restoring_does_not_take_a_name_a_recycle_bin_undo_reserves() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "trashed").unwrap();
        let id = trash_note(&t.0, "a.md");
        fs::write(t.0.join("a.md"), "recycled").unwrap();
        assert_eq!(delete(&t.0, &t.0.join("bin"), "a.md"), Ok(true));
        assert_eq!(restore_trashed_in(&t.0, &id, false), Ok("a (2).md".into()));
        assert_eq!(undo_delete_in(&t.0, "a.md"), Ok(()));
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "recycled");
    }
    #[test]
    fn a_numbered_name_near_the_length_limit_is_cut_to_fit() {
        let _serial = deleting();
        let t = Temp::new();
        let name = format!("{}.md", "n".repeat(250));
        fs::write(t.0.join(&name), "old").unwrap();
        let id = trash_note(&t.0, &name);
        fs::write(t.0.join(&name), "new").unwrap();
        let restored = restore_trashed_in(&t.0, &id, false).unwrap();
        assert!(
            restored.ends_with(" (2).md") && restored.len() <= 255,
            "{restored}"
        );
        assert_eq!(fs::read_to_string(t.0.join(&restored)).unwrap(), "old");
    }
    /// A notes folder in `t`, with its marker, and `files` in it, each with the folders above it; the
    /// Recycle Bin of `to_bin` stays outside, so what lands there is no note of the folder.
    fn folder_with(t: &Temp, files: &[(&str, &[u8])]) -> PathBuf {
        let root = t.0.join("notes");
        fs::create_dir(&root).unwrap();
        stamp(&root).unwrap();
        for (path, content) in files {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, content).unwrap();
        }
        root
    }
    /// A notes folder where `a.md` uses the one image `attachments/map.png`.
    fn note_with_image(t: &Temp) -> PathBuf {
        folder_with(
            t,
            &[
                ("a.md", b"![](attachments/map.png)"),
                ("attachments/map.png", b"map"),
            ],
        )
    }
    fn delete_to(root: &Path, bin: &Path, path: &str) -> Removal {
        delete_note_bin(root, path, |file| to_bin(file, bin)).unwrap()
    }
    /// The UI saved what the panel left open and asks for the images pending, which go through
    /// `recycle`.
    fn send(root: &Path, session: u64, recycle: impl Fn(&Path) -> io::Result<Option<PathBuf>>) {
        send_pending_in(root, ended_pending(session), folder_choices(), recycle);
    }
    /// The panel hides, then the UI asks for the images pending.
    fn hide(root: &Path, recycle: impl Fn(&Path) -> io::Result<Option<PathBuf>>) {
        let session = forget_deleted();
        send(root, session, recycle);
    }
    fn nothing_moves(_: &Path) -> io::Result<Option<PathBuf>> {
        panic!("nothing should move")
    }
    /// Deletes `a.md` to the trash and hides the panel, so its image joins its entry, whose id it
    /// answers.
    fn trash_with_image(root: &Path) -> String {
        let id = trash_note(root, "a.md");
        hide(root, refused_bin);
        id
    }
    #[test]
    fn the_images_only_a_deleted_note_used_go_when_the_panel_hides() {
        let _serial = deleting();
        let t = Temp::new();
        let root = folder_with(
            &t,
            &[
                (
                    "a.md",
                    b"![](attachments/only.png) ![](attachments/shared.png) ![](other/loose.png)",
                ),
                ("b.md", b"Also see SHARED.png."),
                ("attachments/only.png", b"only"),
                ("attachments/shared.png", b"shared"),
                ("other/loose.png", b"loose"),
            ],
        );
        let bin = t.0.join("bin");
        assert_eq!(delete_to(&root, &bin, "a.md").images, 1);
        assert!(exists(&root.join("attachments/only.png")));
        hide(&root, |file| to_bin(file, &bin));
        assert!(!exists(&root.join("attachments/only.png")));
        assert_eq!(fs::read(bin.join("only.png")).unwrap(), b"only");
        assert!(exists(&root.join("attachments/shared.png")));
        assert!(exists(&root.join("other/loose.png")));
    }
    #[test]
    fn undo_before_the_panel_hides_moves_no_image() {
        let _serial = deleting();
        for to_trash in [false, true] {
            let t = Temp::new();
            let root = note_with_image(&t);
            if to_trash {
                let id = trash_note(&root, "a.md");
                assert_eq!(restore_trashed_in(&root, &id, true), Ok("a.md".into()));
            } else {
                assert_eq!(delete_to(&root, &t.0.join("bin"), "a.md").images, 1);
                assert_eq!(undo_delete_in(&root, "a.md"), Ok(()));
            }
            hide(&root, nothing_moves);
            assert_eq!(fs::read(root.join("attachments/map.png")).unwrap(), b"map");
        }
    }
    #[test]
    fn an_image_another_note_uses_when_the_panel_hides_stays() {
        let _serial = deleting();
        let t = Temp::new();
        let root = note_with_image(&t);
        assert_eq!(delete_to(&root, &t.0.join("bin"), "a.md").images, 1);
        fs::write(root.join("b.md"), "![](attachments/map.png)").unwrap();
        hide(&root, nothing_moves);
        assert_eq!(fs::read(root.join("attachments/map.png")).unwrap(), b"map");
    }
    #[test]
    fn an_image_a_note_in_the_trash_references_stays() {
        let _serial = deleting();
        let t = Temp::new();
        let link = b"![](attachments/map.png)" as &[u8];
        let root = folder_with(
            &t,
            &[
                ("a.md", link),
                ("old.md", link),
                ("attachments/map.png", b"map"),
            ],
        );
        trash_note(&root, "old.md");
        assert_eq!(delete_to(&root, &t.0.join("bin"), "a.md").images, 0);
        hide(&root, nothing_moves);
        assert!(exists(&root.join("attachments/map.png")));
    }
    #[test]
    fn a_note_that_cannot_be_read_keeps_the_images() {
        let _serial = deleting();
        let t = Temp::new();
        let root = folder_with(
            &t,
            &[
                ("a.md", b"![](attachments/map.png)"),
                ("b.md", b"\xff not UTF-8"),
                ("attachments/map.png", b"map"),
            ],
        );
        let removal = delete_to(&root, &t.0.join("bin"), "a.md");
        assert_eq!((removal.undo, removal.images), (true, 0));
        hide(&root, nothing_moves);
        assert!(exists(&root.join("attachments/map.png")));
    }
    #[test]
    fn the_folder_going_away_keeps_the_pending_images() {
        let _serial = deleting();
        let t = Temp::new();
        let root = note_with_image(&t);
        assert_eq!(delete_to(&root, &t.0.join("bin"), "a.md").images, 1);
        forget_unavailable();
        hide(&root, nothing_moves);
        assert_eq!(fs::read(root.join("attachments/map.png")).unwrap(), b"map");
    }
    #[test]
    fn a_note_in_the_trash_takes_its_images_into_its_entry_and_brings_them_back() {
        let _serial = deleting();
        let t = Temp::new();
        let root = folder_with(
            &t,
            &[
                ("sub/a.md", b"![](attachments/map.png)"),
                ("sub/attachments/map.png", b"map"),
            ],
        );
        let removal = delete_note_bin(&root, "sub/a.md", refused_bin).unwrap();
        assert_eq!(removal.images, 1);
        let id = removal.trash.unwrap();
        hide(&root, refused_bin);
        assert!(!exists(&root.join("sub/attachments/map.png")));
        let image = trash_of(&root)
            .join(&id)
            .join(".images/sub/attachments/map.png");
        assert_eq!(fs::read(image).unwrap(), b"map");
        let items = trash_items_in(&root).unwrap();
        let shape: Vec<_> = items
            .iter()
            .map(|i| (i.id.as_str(), i.path.as_deref()))
            .collect();
        assert_eq!(shape, [(id.as_str(), Some("sub/a.md"))]);
        assert_eq!(restore_trashed_in(&root, &id, false), Ok("sub/a.md".into()));
        assert!(exists(&root.join("sub/a.md")));
        assert_eq!(
            fs::read(root.join("sub/attachments/map.png")).unwrap(),
            b"map"
        );
        assert!(!exists(&trash_of(&root).join(&id)));
    }
    #[test]
    fn an_image_whose_name_is_taken_stays_in_its_entry_which_lists_and_restores_it() {
        let _serial = deleting();
        let t = Temp::new();
        let root = note_with_image(&t);
        let id = trash_with_image(&root);
        fs::write(root.join("attachments/map.png"), "other").unwrap();
        assert_eq!(
            restore_trashed_in(&root, &id, false),
            Err(Error::ImagesLeftInTrash {
                path: "a.md".into(),
                images: "attachments/map.png".into(),
                count: 1,
            })
        );
        assert!(exists(&root.join("a.md")));
        assert_eq!(
            fs::read(root.join("attachments/map.png")).unwrap(),
            b"other"
        );
        assert!(!exists(&root.join("attachments/map (2).png")));
        let [item] = &trash_items_in(&root).unwrap()[..] else {
            panic!("the image should stay as one item");
        };
        assert_eq!(item.path.as_deref(), Some("attachments/map.png"));
        fs::remove_file(root.join("attachments/map.png")).unwrap();
        assert_eq!(
            restore_trashed_in(&root, &item.id, false),
            Ok("attachments/map.png".into())
        );
        assert_eq!(fs::read(root.join("attachments/map.png")).unwrap(), b"map");
        assert!(trash_items_in(&root).unwrap().is_empty());
    }
    #[test]
    fn restoring_never_looks_through_a_link_that_took_an_images_folder() {
        let _serial = deleting();
        let t = Temp::new();
        let root = note_with_image(&t);
        let id = trash_with_image(&root);
        fs::remove_dir(root.join("attachments")).unwrap();
        fs::create_dir(t.0.join("outside")).unwrap();
        assert!(mklink(
            &["/J"],
            &root.join("attachments"),
            &t.0.join("outside")
        ));
        assert_eq!(
            restore_trashed_in(&root, &id, false),
            Err(Error::ImagesLeftInTrash {
                path: "a.md".into(),
                images: "attachments/map.png".into(),
                count: 1,
            })
        );
        assert_eq!(fs::read_dir(t.0.join("outside")).unwrap().count(), 0);
    }
    #[test]
    fn images_wait_for_the_panel_session_of_their_delete_to_end() {
        let _serial = deleting();
        let t = Temp::new();
        let root = note_with_image(&t);
        let bin = t.0.join("bin");
        let earlier = deleted().session.wrapping_sub(1);
        assert_eq!(delete_to(&root, &bin, "a.md").images, 1);
        // A late request for a session that ended before this delete.
        send(&root, earlier, nothing_moves);
        assert!(exists(&root.join("attachments/map.png")));
        hide(&root, |file| to_bin(file, &bin));
        assert!(!exists(&root.join("attachments/map.png")));
    }
    #[test]
    fn a_late_request_leaves_the_images_of_a_later_session() {
        let _serial = deleting();
        let t = Temp::new();
        let root = note_with_image(&t);
        let first = forget_deleted();
        assert_eq!(delete_to(&root, &t.0.join("bin"), "a.md").images, 1);
        // The second hide ended; the first hide's request arrives only now.
        let _ = forget_deleted();
        send(&root, first, nothing_moves);
        assert!(exists(&root.join("attachments/map.png")));
    }
    #[test]
    fn a_send_that_started_before_a_folder_was_chosen_moves_nothing() {
        let t = Temp::new();
        let id = stamp(&t.0).unwrap();
        fs::create_dir(t.0.join("attachments")).unwrap();
        fs::write(t.0.join("attachments/map.png"), b"map").unwrap();
        // A count from before a choice; changing the real one would race the other tests.
        let before = folder_choices().wrapping_sub(1);
        let paths = ["attachments/map.png".to_owned()];
        assert_eq!(
            send_images_in(&t.0, &paths, None, (&id, before), nothing_moves),
            Err(Error::ImagesFolderChanged)
        );
        assert!(exists(&t.0.join("attachments/map.png")));
    }
    #[test]
    fn a_folder_whose_marker_changed_keeps_the_pending_images() {
        let _serial = deleting();
        let t = Temp::new();
        let root = note_with_image(&t);
        assert_eq!(delete_to(&root, &t.0.join("bin"), "a.md").images, 1);
        fs::write(root.join(DERIVED).join(MARKER), new_folder_id()).unwrap();
        hide(&root, nothing_moves);
        assert_eq!(fs::read(root.join("attachments/map.png")).unwrap(), b"map");
    }
    #[test]
    fn an_image_its_notes_entry_cannot_take_stays_in_place() {
        let _serial = deleting();
        let t = Temp::new();
        let root = note_with_image(&t);
        let id = trash_note(&root, "a.md");
        // A file where the entry's `.images` folder would go.
        fs::write(trash_of(&root).join(&id).join(".images"), "").unwrap();
        hide(&root, nothing_moves);
        assert_eq!(fs::read(root.join("attachments/map.png")).unwrap(), b"map");
        assert_eq!(trash_items_in(&root).unwrap().len(), 1);
    }
    #[test]
    fn a_stray_file_among_an_entrys_images_never_holds_back_the_rest() {
        let _serial = deleting();
        let t = Temp::new();
        let root = note_with_image(&t);
        let id = trash_with_image(&root);
        let images = trash_of(&root).join(&id).join(".images/attachments");
        fs::write(images.join("map.png.tmp"), "partial").unwrap();
        assert_eq!(
            restore_trashed_in(&root, &id, false),
            Err(Error::ImagesLeftInTrash {
                path: "a.md".into(),
                images: "attachments/map.png.tmp".into(),
                count: 1,
            })
        );
        assert!(exists(&root.join("a.md")));
        assert_eq!(fs::read(root.join("attachments/map.png")).unwrap(), b"map");
    }
    #[test]
    fn images_left_by_a_partial_restore_start_their_30_days_again() {
        let _serial = deleting();
        let t = Temp::new();
        let root = note_with_image(&t);
        let id = trash_with_image(&root);
        let before = trash::now_ms();
        fs::write(root.join("attachments/map.png"), "other").unwrap();
        assert!(restore_trashed_in(&root, &id, false).is_err());
        let [item] = &trash_items_in(&root).unwrap()[..] else {
            panic!("the image should stay as one item");
        };
        assert_ne!(item.id, id);
        assert!(item.deleted >= before);
    }
    #[test]
    fn an_entry_from_before_images_still_lists_and_restores_its_one_item() {
        let _serial = deleting();
        let t = Temp::new();
        plant(&t.0, "1-0-0-1", "sub/a.md", "a");
        let items = trash_items_in(&t.0).unwrap();
        let shape: Vec<_> = items
            .iter()
            .map(|i| (i.id.as_str(), i.path.as_deref()))
            .collect();
        assert_eq!(shape, [("1-0-0-1", Some("sub/a.md"))]);
        assert_eq!(
            restore_trashed_in(&t.0, "1-0-0-1", false),
            Ok("sub/a.md".into())
        );
        assert_eq!(fs::read_to_string(t.0.join("sub/a.md")).unwrap(), "a");
    }
    #[test]
    fn a_too_large_note_goes_to_the_trash_but_not_to_the_recycle_bin() {
        let _serial = deleting();
        let t = Temp::new();
        let big = vec![b'a'; MAX_NOTE_BYTES as usize + 1];
        fs::write(t.0.join("big.md"), &big).unwrap();
        let refused = delete_note_in(&t.0, "big.md", |_| unreachable!(), |_| Ok(true));
        assert!(
            matches!(refused, Err(Error::TooLarge { .. })),
            "{refused:?}"
        );
        let removal = delete_note_in(&t.0, "big.md", refused_bin, |_| Ok(false)).unwrap();
        assert!(removal.trash.is_some());
        assert!(!exists(&t.0.join("big.md")));
    }
    #[test]
    fn a_vault_locking_forgets_the_pinned_items_kept_for_undo() {
        let _serial = deleting();
        let t = Temp::new();
        let (root, _) = pinned_setup(&t);
        fs::write(root.join("a.md"), "a").unwrap();
        fs::write(root.join(PINNED), "- [A](a.md)\n").unwrap();
        let id = trash_note(&root, "a.md");
        forget_unavailable();
        assert!(deleted().trashed.is_empty());
        // The item still comes back; only its pinned items do not.
        assert_eq!(restore_trashed_in(&root, &id, true), Ok("a.md".into()));
        assert!(
            !fs::read_to_string(root.join(PINNED))
                .unwrap()
                .contains("a.md")
        );
    }
    #[test]
    fn a_folder_in_the_recycle_bin_offers_no_undo() {
        let t = Temp::new();
        fs::create_dir(t.0.join("folder")).unwrap();
        fs::write(t.0.join("folder/a.md"), "a").unwrap();
        let bin = t.0.join("bin");
        assert_eq!(
            delete_folder_in(&t.0, "folder", |p| to_bin(p, &bin)),
            Ok(Removal::default())
        );
        assert!(exists(&bin.join("folder/a.md")));
    }
    #[test]
    fn renaming_only_the_case_keeps_the_same_file() {
        let t = Temp::new();
        fs::write(t.0.join("note.md"), "text").unwrap();
        assert_eq!(rename(&t.0, "note.md", "Note.md"), Ok(()));
        let paths: Vec<_> = list(&t.0)
            .unwrap()
            .notes
            .into_iter()
            .map(|n| n.path)
            .collect();
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
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
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
    // Catch plaintext migration loss, writes under locked mounts, and retry overwrites.
    #[test]
    fn legacy_derived_data_moves_before_sources_are_removed() {
        let t = Temp::new();
        let (root, store) = pinned_setup(&t);
        let settings = chosen(
            &t,
            &root,
            serde_json::json!({ "collapsed": ["secret.md"], "theme": "dark" }),
        );
        let legacy = settings.with_file_name(PINNED_COPY_FILE);
        fs::write(
            &legacy,
            serde_json::to_vec(&PinnedCopy {
                folder: root.clone(),
                text: "- [Secret](secret.md)\n".into(),
            })
            .unwrap(),
        )
        .unwrap();
        migrate_derived(&root, &settings).unwrap();
        assert!(!legacy.exists());
        assert_eq!(read_copy(&store).unwrap().text, "- [Secret](secret.md)\n");
        assert_eq!(
            read_collapsed(store.parent().unwrap()).unwrap(),
            BTreeSet::from(["secret.md".into()])
        );
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(settings).unwrap()).unwrap();
        assert!(value.get("collapsed").is_none());
        assert_eq!(value["theme"], "dark");
    }
    /// Regression: migrating into another folder at the chosen path, such as the empty local folder a
    /// locked vault leaves at its mount point, wrote pinned names in plaintext outside the vault.
    #[test]
    fn migration_skips_a_folder_with_another_marker() {
        let t = Temp::new();
        let root = t.0.join("notes");
        fs::create_dir(&root).unwrap();
        let settings = chosen(&t, &root, serde_json::json!({ "collapsed": ["secret.md"] }));
        let other = new_folder_id();
        fs::write(root.join(DERIVED).join(MARKER), &other).unwrap();
        migrate_derived(&root, &settings).unwrap();
        assert_eq!(tree(&root), [".docdock", ".docdock/folder-id"]);
        assert_eq!(
            load_value(&settings)["collapsed"],
            serde_json::json!(["secret.md"])
        );
    }
    #[test]
    fn unavailable_migration_keeps_sources_and_creates_nothing() {
        let t = Temp::new();
        let settings = t.0.join("settings.json");
        let legacy = t.0.join(PINNED_COPY_FILE);
        // Invalid bytes prove the copy was not read/deserialized while the folder was away.
        fs::write(&legacy, "unread legacy content").unwrap();
        fs::write(&settings, r#"{"collapsed":["secret.md"]}"#).unwrap();
        for root in [t.0.join("missing"), t.0.join("offline/notes")] {
            migrate_derived(&root, &settings).unwrap();
            assert!(!root.exists());
            assert!(!root.parent().unwrap().join(".docdock").exists());
        }
        assert_eq!(fs::read_to_string(legacy).unwrap(), "unread legacy content");
        assert_eq!(
            fs::read_to_string(settings).unwrap(),
            r#"{"collapsed":["secret.md"]}"#
        );
    }
    #[test]
    fn preference_save_retains_pending_collapsed_migration() {
        let t = Temp::new();
        let settings = t.0.join("settings.json");
        let root = t.0.join("offline/notes");
        fs::write(
            &settings,
            serde_json::to_vec(&serde_json::json!({
                "folder": root, "collapsed": ["secret.md"]
            }))
            .unwrap(),
        )
        .unwrap();
        let mut prefs = load(&settings);
        prefs.theme = Theme::Dark;
        save(&settings, &prefs).unwrap();
        let before = fs::read(&settings).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&before).unwrap();
        assert_eq!(value["collapsed"], serde_json::json!(["secret.md"]));
        assert!(!root.exists());
    }
    #[test]
    fn changing_folders_discards_pending_collapsed_migration() {
        let t = Temp::new();
        let settings = t.0.join("settings.json");
        let root = t.0.join("offline/notes");
        fs::write(
            &settings,
            serde_json::to_vec(&serde_json::json!({
                "folder": root, "collapsed": ["secret.md"]
            }))
            .unwrap(),
        )
        .unwrap();
        let mut prefs = load(&settings);
        prefs.folder = Some(t.0.join("other"));
        save(&settings, &prefs).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
        assert!(value.get("collapsed").is_none());
        assert!(!root.exists());
    }
    #[test]
    fn migration_retry_preserves_newer_destination_state() {
        let t = Temp::new();
        let (root, store) = pinned_setup(&t);
        let settings = chosen(&t, &root, serde_json::json!({ "collapsed": ["old.md"] }));
        let dir = store.parent().unwrap();
        fs::write(dir.join(COLLAPSED_FILE), r#"["new.md"]"#).unwrap();
        migrate_derived(&root, &settings).unwrap();
        assert_eq!(
            read_collapsed(dir).unwrap(),
            BTreeSet::from(["new.md".into()])
        );
    }
    #[test]
    fn synced_copy_from_another_mount_wins_over_legacy_copy() {
        let t = Temp::new();
        let (root, store) = pinned_setup(&t);
        let settings = chosen(&t, &root, serde_json::json!({}));
        let legacy = settings.with_file_name(PINNED_COPY_FILE);
        fs::write(
            &legacy,
            serde_json::to_vec(&PinnedCopy {
                folder: root.clone(),
                text: "old".into(),
            })
            .unwrap(),
        )
        .unwrap();
        fs::write(
            &store,
            serde_json::to_vec(&PinnedCopy {
                folder: PathBuf::from("another-computer"),
                text: "synced".into(),
            })
            .unwrap(),
        )
        .unwrap();
        migrate_derived(&root, &settings).unwrap();
        assert!(!legacy.exists());
        assert_eq!(restorable_copy(&store).unwrap(), "synced");
    }
    #[test]
    fn failed_migration_keeps_legacy_copy() {
        let t = Temp::new();
        let (root, store) = pinned_setup(&t);
        let settings = chosen(&t, &root, serde_json::json!({}));
        let legacy = t.0.join(PINNED_COPY_FILE);
        fs::write(
            &legacy,
            serde_json::to_vec(&PinnedCopy {
                folder: root.clone(),
                text: "secret".into(),
            })
            .unwrap(),
        )
        .unwrap();
        fs::create_dir(&store).unwrap();
        assert!(migrate_derived(&root, &settings).is_err());
        assert!(legacy.exists());
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
            !pinned_state(
                &other,
                &held_derived_dir(&other).unwrap().0.join(PINNED_COPY_FILE),
                &no_collapsed()
            )
            .unwrap()
            .restorable
        );
        restore_pinned_in(
            &other,
            &held_derived_dir(&other).unwrap().0.join(PINNED_COPY_FILE),
        )
        .unwrap();
        assert!(!other.join(PINNED).exists());
    }
    #[test]
    fn missing_or_corrupt_settings_read_as_the_defaults() {
        let t = Temp::new();
        let p = t.0.join("settings.json");
        assert!(read_settings(&p).is_none());
        fs::write(&p, "{").unwrap();
        assert!(read_settings(&p).is_none());
    }
    #[test]
    fn saving_settings_replaces_the_previous_file() {
        let t = Temp::new();
        let p = t.0.join("settings.json");
        let settings = |name| Settings {
            folder: Some(t.0.join(name)),
            ..Settings::default()
        };
        save(&p, &settings("a")).unwrap();
        save(&p, &settings("b")).unwrap();
        assert_eq!(read_settings(&p), Some(t.0.join("b")));
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 1);
    }
    /// Catches the shared walker handing folders to the leftover scan: a folder named like a backup
    /// was listed as recoverable, and restoring it could only fail.
    #[test]
    fn a_folder_named_like_a_backup_is_not_a_leftover() {
        let t = Temp::new();
        fs::create_dir(t.0.join("a.md.7-1.docdock-bak")).unwrap();
        fs::write(t.0.join("b.md.7-2.docdock-bak"), "b").unwrap();
        let found = leftovers_in(&t.0).unwrap();
        let paths: Vec<_> = found.iter().map(|l| &*l.path).collect();
        assert_eq!(paths, ["b.md.7-2.docdock-bak"]);
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
    #[test]
    fn a_restored_leftover_does_not_take_a_name_a_recycle_bin_undo_reserves() {
        let _serial = deleting();
        let t = Temp::new();
        fs::write(t.0.join("a.md"), "recycled").unwrap();
        fs::write(t.0.join("a.md.7-1.docdock-bak"), "backup").unwrap();
        assert_eq!(delete(&t.0, &t.0.join("bin"), "a.md"), Ok(true));
        assert_eq!(
            restore_in(&t.0, "a.md.7-1.docdock-bak", "r"),
            Ok("a (r).md".into())
        );
        assert_eq!(undo_delete_in(&t.0, "a.md"), Ok(()));
        assert_eq!(fs::read_to_string(t.0.join("a.md")).unwrap(), "recycled");
    }
    #[test]
    fn a_folder_is_missing_only_when_its_parent_lists_without_it() {
        use Availability::*;
        use Probe::*;
        assert_eq!(availability(Folder, None, true), Available);
        assert_eq!(availability(Absent, Some(Folder), false), Missing);
        // The parent lists the name, but it does not open: a link or mount point whose target left.
        assert_eq!(availability(Absent, Some(Folder), true), Unavailable);
        // A vault's mount point or another folder above it is gone.
        assert_eq!(availability(Absent, Some(Absent), false), Unavailable);
        // The volume root itself: a drive letter or share that left.
        assert_eq!(availability(Absent, None, false), Unavailable);
        assert_eq!(availability(Unreadable, Some(Folder), false), Unavailable);
        assert_eq!(availability(Absent, Some(Unreadable), false), Unavailable);
    }
    #[test]
    fn probing_tells_a_deleted_folder_from_one_whose_parent_left() {
        let t = Temp::new();
        assert_eq!(folder_availability(&t.0), Availability::Available);
        assert_eq!(
            folder_availability(&t.0.join("notes")),
            Availability::Missing
        );
        assert_eq!(
            folder_availability(&t.0.join("vault/notes")),
            Availability::Unavailable
        );
        fs::write(t.0.join("file"), "").unwrap();
        assert_eq!(
            folder_availability(&t.0.join("file")),
            Availability::Unavailable
        );
    }
    #[test]
    fn a_junction_whose_target_left_is_unavailable_not_missing() {
        let t = Temp::new();
        fs::create_dir(t.0.join("vault")).unwrap();
        assert!(mklink(&["/J"], &t.0.join("notes"), &t.0.join("vault")));
        fs::remove_dir(t.0.join("vault")).unwrap();
        assert_eq!(
            folder_availability(&t.0.join("notes")),
            Availability::Unavailable
        );
    }
    /// Regression: an ACL can allow reading attributes but refuse listing; such a folder was available.
    #[test]
    fn a_folder_that_refuses_listing_is_unavailable() {
        let t = Temp::new();
        let notes = t.0.join("notes");
        fs::create_dir(&notes).unwrap();
        let _refused = Unlistable::new(&notes);
        assert_eq!(folder_availability(&notes), Availability::Unavailable);
    }
    #[test]
    fn choosing_a_folder_adopts_its_marker() {
        let t = Temp::new();
        let id = new_folder_id();
        fs::create_dir(t.0.join(DERIVED)).unwrap();
        fs::write(t.0.join(DERIVED).join(MARKER), &id).unwrap();
        let settings = t.0.join("config").join("settings.json");
        choose_in(&settings, t.0.clone()).unwrap();
        assert_eq!(load(&settings).folder_id, Some(id.clone()));
        assert_eq!(read_marker(&t.0).unwrap(), Some(id));
    }
    #[test]
    fn choosing_a_folder_that_is_not_there_creates_nothing() {
        let t = Temp::new();
        let settings = t.0.join("settings.json");
        let root = t.0.join("vault").join("notes");
        choose_in(&settings, root.clone()).unwrap();
        assert_eq!(load(&settings).folder, Some(root));
        assert_eq!(load(&settings).folder_id, None);
        assert_eq!(tree(&t.0), ["settings.json"]);
    }
    /// Regression: another folder at the chosen path, such as the empty folder a locked vault leaves
    /// at its mount point or another vault with the same file system, received plaintext.
    #[test]
    fn a_folder_with_another_marker_is_unrecognized_and_untouched() {
        let t = Temp::new();
        let root = t.0.join("notes");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a.md"), "mine").unwrap();
        let settings = chosen(&t, &root, serde_json::json!({ "collapsed": ["secret.md"] }));
        fs::write(root.join(DERIVED).join(MARKER), new_folder_id()).unwrap();
        let before = fs::read(&settings).unwrap();
        assert_eq!(root_in(&settings).err(), Some(Error::FolderUnrecognized));
        assert_eq!(folder_in(&settings).unwrap().1, Availability::Unrecognized);
        assert_eq!(tree(&root), [".docdock", ".docdock/folder-id", "a.md"]);
        assert_eq!(fs::read(&settings).unwrap(), before);
    }
    /// Regression: settings from before markers accepted any folder at the path, so the empty folder
    /// a locked vault leaves at its mount point received the migrated pinned names in plaintext.
    #[test]
    fn an_empty_folder_from_earlier_settings_is_unrecognized_and_untouched() {
        let t = Temp::new();
        let root = t.0.join("notes");
        fs::create_dir(&root).unwrap();
        let settings = earlier_settings(
            &t,
            serde_json::json!({ "folder": root, "collapsed": ["secret.md"] }),
        );
        let before = fs::read(&settings).unwrap();
        assert_eq!(root_in(&settings).err(), Some(Error::FolderUnrecognized));
        assert!(tree(&root).is_empty());
        assert_eq!(fs::read(&settings).unwrap(), before);
    }
    /// Regression: a folder with old plaintext notes left at a locked vault's mount point was taken
    /// for the vault and received new writes in plaintext.
    #[test]
    fn an_unmarked_folder_with_notes_from_earlier_settings_is_unrecognized() {
        let t = Temp::new();
        let root = t.0.join("notes");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a.md"), "old").unwrap();
        let settings = earlier_settings(&t, serde_json::json!({ "folder": root }));
        assert_eq!(root_in(&settings).err(), Some(Error::FolderUnrecognized));
        assert_eq!(tree(&root), ["a.md"]);
    }
    #[test]
    fn a_folder_marked_elsewhere_is_adopted_from_earlier_settings() {
        let t = Temp::new();
        let root = t.0.join("notes");
        fs::create_dir_all(root.join(DERIVED)).unwrap();
        let id = new_folder_id();
        fs::write(root.join(DERIVED).join(MARKER), &id).unwrap();
        let settings = earlier_settings(&t, serde_json::json!({ "folder": root }));
        assert!(root_in(&settings).is_ok());
        assert_eq!(load(&settings).folder_id, Some(id));
    }
    #[test]
    fn trusting_the_configured_folder_marks_it() {
        let t = Temp::new();
        let root = t.0.join("notes");
        fs::create_dir(&root).unwrap();
        let settings = earlier_settings(&t, serde_json::json!({ "folder": root }));
        trust_in(&settings).unwrap();
        assert!(root_in(&settings).is_ok());
        assert_eq!(load(&settings).folder_id, read_marker(&root).unwrap());
    }
    /// Regression: a folder created under a locked vault's mount point puts plaintext on the
    /// unencrypted disk, and the vault can no longer mount there.
    #[test]
    fn nothing_creates_a_notes_folder_that_is_not_there() {
        let t = Temp::new();
        let store = t.0.join("config").join(PINNED_COPY_FILE);
        let png = b"\x89PNG\r\n\x1a\n";
        for root in [t.0.join("vault").join("notes"), t.0.join("notes")] {
            keep_copy(&store, &root, "- [a](a.md)\n");
            assert!(save_note_in(&root, "a.md", "secret", "").is_err());
            assert!(save_note_in(&root, "sub/a.md", "secret", "v").is_err());
            assert!(!matches!(
                write_note_in(&root, "a.md", "secret", "v", false),
                Ok(Saved::Saved { .. })
            ));
            assert!(create_folder_in(&root, "sub").is_err());
            assert!(
                edit_pinned(&root, |text| pinned::pin(
                    text,
                    Kind::Url,
                    "https://a.b/",
                    "a"
                ))
                .is_err()
            );
            assert!(restore_pinned_in(&root, &store).is_err());
            assert!(paste_image_in(&root, "a.md", png).is_err());
            assert!(restore_in(&root, "a.md.1-1.docdock-bak", "r").is_err());
            assert!(move_note_in(&root, "a.md", "sub/a.md", "v").is_err());
            assert!(rename_note_in(&root, "a.md", "b.md", "v", ("", "")).is_err());
            let created: Vec<_> = fs::read_dir(&t.0)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            assert!(created.is_empty());
        }
    }
}
