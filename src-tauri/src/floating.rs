//! Floating windows: note images and notes (post-its) detached from the panel into borderless,
//! topmost windows that never activate (but a post-it while its note is edited, which happens in
//! one place at a time: `Registry::may_edit`), with their own opacity and a click-through mode,
//! reopened where they were from `.docdock/floating.json`. The rules are pure in `floats.rs`; this
//! runs their effects. Each window is a secondary app window
//! ([0014](../../docs/decisions/0014-app-windows.md)) labeled `float-<n>` for this run: it has no
//! capability, reaches only the `float_*` commands, and only for itself, and hears what to show
//! only through the IPC channel it hands over with `float_state`.
//!
//! Every window belongs to the notes folder it opened on (`notes::FolderIdentity`, its path and
//! marker id) and to the registry's epoch, which changes whenever that folder leaves or another is
//! chosen: work started on an earlier epoch never registers, shows or saves a window, and anything
//! it would write must name the folder still configured.
//!
//! One worker thread saves moves and fades after a pause and, while any window lets clicks
//! through, watches Alt and the cursor so holding Alt over such a window reaches its controls.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::dock::{EditNoteRequest, Request};
use crate::error::Error;
use crate::floats::{self, Entry, Kind, MAX_FLOATING};
use crate::i18n::Locale;
use crate::notes::{FolderIdentity, OpenNote};
use crate::platform::{self, Point, Rect, scale_dpi};

/// How often Alt and the cursor are read while a window lets clicks through.
const POLL: Duration = Duration::from_millis(50);
/// Moves and fades are saved once they pause this long.
const SAVE_DELAY: Duration = Duration::from_millis(500);
/// How long exiting waits for the last moves to be saved, which may wait for a note being saved.
const FLUSH_WAIT: Duration = Duration::from_secs(10);
/// How long opening waits for the panel thread to say where the window goes.
const PLACE_WAIT: Duration = Duration::from_secs(1);

/// What a floating window shows: the highest `version` wins, whichever way it arrived.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    version: u64,
    kind: Kind,
    path: Option<String>,
    /// Changes whenever a post-it's note may have changed on disk: the window reads it again.
    revision: u64,
    opacity: f64,
    text_scale: f64,
    click_through: bool,
    /// Alt is held over the click-through window: it takes clicks and shows its controls.
    revealed: bool,
    /// The post-it shows only its title bar.
    collapsed: bool,
    /// Changes on Escape over a post-it, even while another window has focus.
    escape_revision: u64,
    /// The post-it edits its note (`begin_edit`).
    editing: bool,
    /// The post-it's note is edited elsewhere, in the panel or another post-it.
    blocked: bool,
    /// The post-it was editing when its notes folder left or another was chosen: it keeps its text
    /// but must hide it, and reach its note again only once the same folder returns.
    parked: bool,
    /// Rust asks the editing post-it to finish (`finish_where`): each new request raises it, and
    /// the window answers that number with `float_finished` once it saved, or could not.
    finish: u64,
    /// The user closed the editing post-it from outside it (Alt+F4, the panel, the tray): each
    /// request raises it, and the window closes itself only once nothing is left to save.
    close_requested: u64,
}

#[derive(Serialize)]
pub struct State {
    #[serde(flatten)]
    update: Update,
    locale: Locale,
}

/// A floating window of this run.
struct Live {
    label: String,
    /// Its entry in `floating.json`.
    id: String,
    kind: Kind,
    path: String,
    /// See `Update::revision`.
    revision: u64,
    /// The window, once built and prepared; 0 before.
    raw: isize,
    /// The image's width over its height, which the window keeps when resized or zoomed; shared
    /// with its `WM_SIZING` hook.
    aspect: Aspect,
    /// The post-it shows only its title bar; shared with its `WM_SIZING` hook, which then keeps the
    /// window at the bar's height.
    collapse: Collapse,
    /// A collapsed post-it's height when expanded, in physical pixels, saved in its place.
    expanded_height: i32,
    /// Saved and shown once: until then nothing but opening shows it.
    shown: bool,
    /// The notes folder it opened on, and the registry's epoch then: it is saved only there, and
    /// only while that epoch lasts.
    folder: FolderIdentity,
    epoch: u64,
    /// The user closed it: its entry is being forgotten, by the close or by a revocation that
    /// overtakes it.
    closing: bool,
    opacity: f64,
    /// A post-it's text size against the panel's.
    text_scale: f64,
    click_through: bool,
    revealed: bool,
    escape_revision: u64,
    /// The post-it holds its note's edit lease (`Registry::may_edit`), and takes focus meanwhile.
    editing: bool,
    /// See `Update::blocked`.
    blocked: bool,
    /// See `Update::parked`. It keeps the folder and epoch it opened on, so none of its moves is
    /// saved, until the same folder returns and adopts it (`Registry::adopt`).
    parked: bool,
    /// See `Update::finish`.
    finish: u64,
    /// See `Update::close_requested`.
    close_requested: u64,
    /// The foreground window when editing began, which gets focus back when it ends.
    previous_focus: Option<isize>,
    /// Counts the edits granted to the post-it: a save names the one it belongs to, so a save
    /// still waiting when its edit ended never writes into a later one (`Registry::editing`).
    lease: u64,
    /// Changes with what the window shows.
    version: u64,
    /// The window's channel, from its latest load.
    channel: Option<Channel<Update>>,
}

impl Live {
    fn update(&self) -> Update {
        Update {
            version: self.version,
            kind: self.kind,
            path: Some(self.path.clone()),
            revision: self.revision,
            opacity: self.opacity,
            text_scale: self.text_scale,
            click_through: self.click_through,
            revealed: self.revealed,
            collapsed: self.collapse.get(),
            escape_revision: self.escape_revision,
            editing: self.editing,
            blocked: self.blocked,
            parked: self.parked,
            finish: self.finish,
            close_requested: self.close_requested,
        }
    }

    /// Tells the window what changed.
    fn changed(&mut self) {
        self.version += 1;
        if let Some(channel) = &self.channel
            && let Err(error) = channel.send(self.update())
        {
            eprintln!("[floating] update not delivered: {error}");
        }
    }
}

/// An image's width over its height, read by the window's thread while resizing.
#[derive(Clone)]
struct Aspect(Arc<AtomicU64>);

impl Aspect {
    fn new(aspect: f64) -> Self {
        Self(Arc::new(AtomicU64::new(aspect.to_bits())))
    }

    fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn set(&self, aspect: f64) {
        self.0.store(aspect.to_bits(), Ordering::Relaxed);
    }
}

/// Whether a post-it is collapsed, read by the window's thread while resizing.
#[derive(Clone)]
struct Collapse(Arc<AtomicBool>);

impl Collapse {
    fn new(collapsed: bool) -> Self {
        Self(Arc::new(AtomicBool::new(collapsed)))
    }

    fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn set(&self, collapsed: bool) {
        self.0.store(collapsed, Ordering::Relaxed);
    }
}

/// Where a note is edited: the panel, or the post-it of that label. One at a time per note.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Editor {
    Panel,
    Float(String),
}

/// Why a note may not be edited now.
#[derive(Debug, PartialEq, Eq)]
enum Refusal {
    /// No edit begins now: the app is exiting or another notes folder is being chosen.
    Held,
    /// The note is edited there, which has to finish first.
    Editing(Editor),
}

impl Refusal {
    fn log(&self, path: &str) {
        match self {
            Self::Held => eprintln!("[floating] no edit begins now: {path}"),
            Self::Editing(Editor::Panel) => eprintln!("[floating] edited in the panel: {path}"),
            Self::Editing(Editor::Float(label)) => {
                eprintln!("[floating] edited in {label}: {path}")
            }
        }
    }
}

/// What it takes for an editor to edit a note (`Registry::transfer`). Asking to edit a note edited
/// elsewhere moves the edit: the other editor saves and returns to reading first.
#[derive(Debug, PartialEq, Eq)]
enum Transfer {
    Grant,
    Refuse,
    /// Ask this editor to finish, then ask again.
    Finish(Editor),
}

/// What a post-it that may begin editing reads its note from, and the edit it was granted.
#[derive(Debug, PartialEq)]
struct Claim {
    path: String,
    folder: FolderIdentity,
    lease: u64,
}

/// A note a post-it begins editing, and the edit it was granted, which its saves name.
#[derive(Serialize)]
pub struct Granted {
    #[serde(flatten)]
    note: OpenNote,
    lease: u64,
}

/// An edit that ended: the post-it's window, the window that had focus before, and whether the
/// post-it was parked, which then goes.
#[derive(Debug, PartialEq)]
struct Ended {
    raw: isize,
    previous_focus: Option<isize>,
    parked: bool,
}

/// A window the user closed, whose entry and window are on their way out.
#[derive(Debug, PartialEq)]
struct Closing {
    label: String,
    id: String,
    folder: FolderIdentity,
}

struct Registry {
    /// The last label number given; labels are never reused within a run.
    next: u64,
    /// Changes whenever the windows' notes folder leaves, or another is chosen or trusted.
    epoch: u64,
    /// Folder choices under way: nothing opens until they end.
    switching: usize,
    live: Vec<Live>,
    /// The note the panel edits, relative to the notes folder, as it spelled it.
    panel_editing: Option<String>,
    /// No edit begins, as while the app exits or another folder is chosen.
    held: bool,
}

impl Registry {
    const fn new() -> Self {
        Self {
            next: 0,
            epoch: 0,
            switching: 0,
            live: Vec::new(),
            panel_editing: None,
            held: false,
        }
    }

    /// Whether `editor` may edit the note `path`: nobody else edits it, however spelled, and no
    /// edit is held off. An editor that already edits it may go on. Answers who edits it otherwise,
    /// so it can be asked to finish.
    fn may_edit(&self, editor: &Editor, path: &str) -> Result<(), Refusal> {
        if self.held || self.switching > 0 {
            return Err(Refusal::Held);
        }
        if *editor != Editor::Panel
            && self
                .panel_editing
                .as_deref()
                .is_some_and(|editing| same(editing, path))
        {
            return Err(Refusal::Editing(Editor::Panel));
        }
        let elsewhere = self.live.iter().find(|live| {
            live.editing
                && same(&live.path, path)
                && !matches!(editor, Editor::Float(label) if *label == live.label)
        });
        match elsewhere {
            Some(live) => Err(Refusal::Editing(Editor::Float(live.label.clone()))),
            None => Ok(()),
        }
    }

    /// What it takes for `editor` to edit `path` (`Transfer`): nothing if `may_edit` agrees; never
    /// while edits are held, nor from a parked post-it, which cannot save now; otherwise its editor
    /// finishing first.
    fn transfer(&self, editor: &Editor, path: &str) -> Transfer {
        match self.may_edit(editor, path) {
            Ok(()) => Transfer::Grant,
            Err(Refusal::Held) => Transfer::Refuse,
            Err(Refusal::Editing(Editor::Float(label)))
                if self
                    .live
                    .iter()
                    .any(|live| live.label == label && live.parked) =>
            {
                Transfer::Refuse
            }
            Err(Refusal::Editing(holder)) => Transfer::Finish(holder),
        }
    }

    /// What it takes for the post-it `label` to begin editing: refused, as `begin_edit` would,
    /// before asking anyone else to finish.
    fn begin_transfer(&self, label: &str) -> Result<Transfer, Error> {
        let live = self
            .live
            .iter()
            .find(|live| live.label == label)
            .ok_or_else(|| Error::unexpected("command restricted to floating windows"))?;
        if live.kind != Kind::Note {
            return Err(Error::unexpected("command restricted to post-its"));
        }
        if live.raw == 0
            || !live.shown
            || live.closing
            || live.click_through
            || live.parked
            || !self.current(live.epoch)
        {
            return Ok(Transfer::Refuse);
        }
        Ok(self.transfer(&Editor::Float(label.to_owned()), &live.path))
    }

    /// Tells every post-it whose note became edited elsewhere, or no longer is.
    fn refresh_blocked(&mut self) {
        let blocked: Vec<bool> = self
            .live
            .iter()
            .map(|live| {
                live.kind == Kind::Note
                    && matches!(
                        self.may_edit(&Editor::Float(live.label.clone()), &live.path),
                        Err(Refusal::Editing(_))
                    )
            })
            .collect();
        for (live, blocked) in self.live.iter_mut().zip(blocked) {
            if live.blocked != blocked {
                live.blocked = blocked;
                live.changed();
            }
        }
    }

    /// The post-it `label` begins editing its note, if it is shown, takes clicks, belongs to the
    /// current epoch and `may_edit` agrees; `None` if not. Refuses floating images and windows it
    /// does not hold.
    fn begin_edit(&mut self, label: &str) -> Result<Option<Claim>, Error> {
        let live = self
            .live
            .iter()
            .find(|live| live.label == label)
            .ok_or_else(|| Error::unexpected("command restricted to floating windows"))?;
        if live.kind != Kind::Note {
            return Err(Error::unexpected("command restricted to post-its"));
        }
        if live.raw == 0 || !live.shown || live.closing || live.click_through || live.parked {
            return Ok(None);
        }
        if !self.current(live.epoch) {
            return Ok(None);
        }
        if let Err(refusal) = self.may_edit(&Editor::Float(label.to_owned()), &live.path) {
            refusal.log(&live.path);
            return Ok(None);
        }
        let Some(live) = self.live.iter_mut().find(|live| live.label == label) else {
            return Ok(None);
        };
        if !live.editing {
            live.editing = true;
            live.lease += 1;
            live.changed();
        }
        let claim = Claim {
            path: live.path.clone(),
            folder: live.folder.clone(),
            lease: live.lease,
        };
        self.refresh_blocked();
        Ok(Some(claim))
    }

    /// The post-it `label` stops editing; answers its window, the window that had focus before and
    /// whether it was parked, if it was editing.
    fn end_edit(&mut self, label: &str) -> Result<Option<Ended>, Error> {
        let live = self
            .live
            .iter_mut()
            .find(|live| live.label == label)
            .ok_or_else(|| Error::unexpected("command restricted to floating windows"))?;
        if live.kind != Kind::Note {
            return Err(Error::unexpected("command restricted to post-its"));
        }
        if !live.editing {
            return Ok(None);
        }
        live.editing = false;
        live.changed();
        let ended = Ended {
            raw: live.raw,
            previous_focus: live.previous_focus.take(),
            parked: live.parked,
        };
        self.refresh_blocked();
        Ok(Some(ended))
    }

    /// The note the post-it `label` edits now, after any rename, and its notes folder, while the
    /// edit `lease` lasts; refused once that edit ended, even if another began since, or the
    /// post-it went.
    fn editing(&self, label: &str, lease: u64) -> Result<(String, FolderIdentity), Error> {
        let live = self
            .live
            .iter()
            .find(|live| live.label == label)
            .ok_or_else(|| Error::unexpected("command restricted to floating windows"))?;
        match live.kind {
            Kind::Image => Err(Error::unexpected("command restricted to post-its")),
            Kind::Note if live.editing && !live.closing && live.lease == lease => {
                Ok((live.path.clone(), live.folder.clone()))
            }
            Kind::Note => Err(Error::unexpected("post-it not editing")),
        }
    }

    /// The post-it `label` may write the clipboard: only while it edits its note and shows it, so a
    /// reading, parked or closing post-it, or a floating image, never reaches the clipboard.
    fn copies(&self, label: &str) -> Result<(), Error> {
        let live = self
            .live
            .iter()
            .find(|live| live.label == label)
            .ok_or_else(|| Error::unexpected("command restricted to floating windows"))?;
        match live.kind {
            Kind::Image => Err(Error::unexpected("command restricted to post-its")),
            Kind::Note if live.editing && !live.closing && !live.parked => Ok(()),
            Kind::Note => Err(Error::unexpected("post-it not editing")),
        }
    }

    /// The panel edits the note `path`, if `may_edit` agrees, or none (`None`, always agreed).
    fn panel_edit(&mut self, path: Option<String>) -> bool {
        if let Some(path) = &path
            && let Err(refusal) = self.may_edit(&Editor::Panel, path)
        {
            refusal.log(path);
            return false;
        }
        self.panel_editing = path;
        self.refresh_blocked();
        true
    }

    /// The notes post-its edit, as saved.
    fn editing_paths(&self) -> Vec<String> {
        let mut paths: Vec<_> = self
            .live
            .iter()
            .filter(|live| live.editing)
            .map(|live| live.path.clone())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// Work started on `epoch` may still open windows: no folder change came since, and none is
    /// under way.
    fn current(&self, epoch: u64) -> bool {
        epoch == self.epoch && self.switching == 0
    }

    /// Takes a label and a place for a window still to be built for `entry`, for work started on
    /// `epoch`; answers its label and how many windows were open already.
    fn reserve(
        &mut self,
        epoch: u64,
        entry: &Entry,
        folder: &FolderIdentity,
    ) -> Result<(String, usize), Error> {
        if !self.current(epoch) {
            return Err(Error::FolderChanged);
        }
        let open = self.live.len();
        if open >= MAX_FLOATING {
            return Err(Error::FloatingLimit { max: MAX_FLOATING });
        }
        self.next += 1;
        let label = floats::label(self.next);
        self.live.push(Live {
            label: label.clone(),
            id: entry.id.clone(),
            kind: entry.kind,
            path: entry.path.clone(),
            revision: 0,
            raw: 0,
            aspect: Aspect::new(1.0),
            collapse: Collapse::new(entry.kind == Kind::Note && entry.collapsed),
            expanded_height: entry.height,
            shown: false,
            folder: folder.clone(),
            epoch,
            closing: false,
            opacity: floats::clamp_opacity(entry.opacity),
            text_scale: floats::clamp_text_scale(entry.text_scale.unwrap_or(1.0)),
            click_through: false,
            revealed: false,
            escape_revision: 0,
            editing: false,
            blocked: false,
            parked: false,
            finish: 0,
            close_requested: 0,
            previous_focus: None,
            lease: 0,
            version: 1,
            channel: None,
        });
        Ok((label, open))
    }

    /// Takes the window `label`, and any edit it held with it.
    fn take(&mut self, label: &str) -> Option<Live> {
        let at = self.live.iter().position(|live| live.label == label)?;
        let live = self.live.remove(at);
        self.refresh_blocked();
        Some(live)
    }

    /// Starts a new epoch and takes every window but the post-its editing, which it parks instead:
    /// they keep their text, hidden, and their label, folder and epoch, so they reach their note
    /// again only in that folder and none of their moves is saved meanwhile. They still hold their
    /// note's lease, by path. The panel's edit was of the folder that left.
    fn revoke(&mut self) -> Vec<Live> {
        self.epoch += 1;
        self.panel_editing = None;
        let (kept, gone) = std::mem::take(&mut self.live)
            .into_iter()
            .partition(|live| live.editing);
        self.live = kept;
        for live in self.live.iter_mut().filter(|live| !live.parked) {
            live.parked = true;
            live.changed();
        }
        gone
    }

    /// The notes folder `folder` returned on the current epoch: the post-its parked from it take
    /// that epoch and read their note again. Those of another folder stay parked until discarded.
    fn adopt(&mut self, folder: &FolderIdentity) {
        let epoch = self.epoch;
        for live in self
            .live
            .iter_mut()
            .filter(|live| live.parked && live.folder == *folder)
        {
            live.parked = false;
            live.epoch = epoch;
            live.revision += 1;
            live.changed();
        }
    }

    /// Marks every window `picked` closed by the user; answers their labels, ids and folders. A
    /// post-it editing is only asked to close (`Update::close_requested`), which it does itself
    /// once nothing is left to save, so a close never loses its text.
    fn mark_closing_where(&mut self, picked: impl Fn(&Live) -> bool) -> Vec<Closing> {
        let mut closing = Vec::new();
        for live in self.live.iter_mut().filter(|live| picked(live)) {
            if live.editing {
                live.close_requested += 1;
                live.changed();
                continue;
            }
            live.closing = true;
            closing.push(Closing {
                label: live.label.clone(),
                id: live.id.clone(),
                folder: live.folder.clone(),
            });
        }
        closing
    }

    /// The images and notes the open windows show, as saved.
    fn paths(&self) -> Vec<String> {
        let mut paths: Vec<_> = self
            .live
            .iter()
            .filter(|live| !live.closing && !live.parked)
            .map(|live| live.path.clone())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// A folder choice begins: as `revoke`, and nothing opens until it ends. The post-its still
    /// editing are parked: a choice from the panel finishes their edits first (`finish_edits`).
    fn begin_switch(&mut self) -> Vec<Live> {
        self.switching += 1;
        self.revoke()
    }

    /// A folder choice ends with a new epoch; answers whether no other is still under way.
    fn end_switch(&mut self) -> bool {
        self.switching = self.switching.saturating_sub(1);
        self.epoch += 1;
        self.switching == 0
    }

    /// The window `label`, opening on `epoch`, may be saved: it is still there and no folder change
    /// came or is under way.
    fn admits(&self, label: &str, epoch: u64) -> Result<(), Error> {
        let here = self
            .live
            .iter()
            .any(|live| live.label == label && !live.closing);
        if self.current(epoch) && here {
            Ok(())
        } else {
            Err(Error::FolderChanged)
        }
    }

    /// The built window `label` and its image's proportions now, if it may show: no close, even
    /// one under way, nor a folder change came since the opening started on `epoch`.
    fn confirm(&self, label: &str, epoch: u64) -> Option<(isize, f64)> {
        if !self.current(epoch) {
            return None;
        }
        self.live
            .iter()
            .find(|live| live.label == label && live.raw != 0 && !live.closing)
            .map(|live| (live.raw, live.aspect.get()))
    }
}

/// The floating windows of this run. Never taken on the main thread: its holders call Win32
/// functions that wait for the main thread, such as changing a window's style. Commands that take
/// it are asynchronous for that reason. Opening checks it under `notes`' `SAVING`, so it is never
/// held while taking that or any other `notes` lock.
static REGISTRY: Mutex<Registry> = Mutex::new(Registry::new());
/// The labels of `REGISTRY`'s windows, for the command barrier and `note-asset`, which run on the
/// main thread. Held only to read or change the set, never around anything else.
static LABELS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
static WORKER: OnceLock<Sender<Message>> = OnceLock::new();
/// Serializes `report`.
static REPORTING: Mutex<()> = Mutex::new(());
/// Serializes `set_collapsed`. Never taken on the main thread nor under the registry.
static COLLAPSING: Mutex<()> = Mutex::new(());
/// Serializes `begin_edit` and `end_edit`, so a post-it never activates after its edit ended.
/// Never taken on the main thread nor under the registry.
static LEASING: Mutex<()> = Mutex::new(());
/// One restore at a time, so two never open the same saved window.
static RESTORING: Mutex<()> = Mutex::new(());

fn lock(registry: &Mutex<Registry>) -> MutexGuard<'_, Registry> {
    registry.lock().unwrap_or_else(|e| e.into_inner())
}

fn registry() -> MutexGuard<'static, Registry> {
    lock(&REGISTRY)
}

fn labels() -> MutexGuard<'static, BTreeSet<String>> {
    LABELS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether `label` is a floating window Rust registered and has not closed.
pub fn is_live(label: &str) -> bool {
    floats::parse_label(label).is_some() && labels().contains(label)
}

/// Registers `label` as a live floating window's, before its window exists, so its page reaches
/// its commands and images from the start.
pub(crate) fn admit(label: &str) {
    labels().insert(label.to_owned());
}

pub(crate) fn forget_label(label: &str) {
    labels().remove(label);
}

/// Runs `action` on the live floating window `label`, which must be the caller's own: a secondary
/// command derives it from its window, never from an argument, so the image preview and windows
/// that closed reach nothing.
fn with_live<T>(label: &str, action: impl FnOnce(&mut Live) -> T) -> Result<T, Error> {
    let mut registry = registry();
    let live = registry
        .live
        .iter_mut()
        .find(|live| live.label == label)
        .ok_or_else(|| Error::unexpected("command restricted to floating windows"))?;
    Ok(action(live))
}

enum Message {
    /// The window moved, was resized or faded: save it once that pauses.
    Changed(String),
    /// A window may have started letting clicks through.
    Wake,
    /// The user closed the window, as with Alt+F4.
    Close(String),
    /// The window is gone, whoever destroyed it; its saved entry stays.
    Destroyed(String),
    UnlockAll,
    /// From the tray: the user closes every window.
    CloseAll,
    /// Save what is waiting now, then answer.
    Flush(Sender<()>),
    /// Something in the notes folder changed: post-its of the folder the watcher saw at the path
    /// follow the renames it saw there, then every post-it reads its note again.
    NotesChanged(Option<std::path::PathBuf>, Vec<(String, String)>),
}

fn send(message: Message) {
    if let Some(worker) = WORKER.get() {
        let _ = worker.send(message);
    }
}

/// A move or fade waiting to be saved.
struct Dirty {
    /// A collapsed post-it's expanded rectangle.
    rect: Rect,
    opacity: f64,
    text_scale: Option<f64>,
    collapsed: Option<bool>,
    folder: FolderIdentity,
    epoch: u64,
}

struct Worker {
    app: AppHandle,
    /// By saved id. A window destroyed before the save, as on exit, is still saved.
    dirty: BTreeMap<String, Dirty>,
    save_at: Option<Instant>,
}

impl Worker {
    fn run(mut self, messages: Receiver<Message>) {
        loop {
            let watching = registry().live.iter().any(|live| live.click_through);
            let now = Instant::now();
            let until_save = self.save_at.map(|at| at.saturating_duration_since(now));
            let wait = if watching {
                Some(until_save.map_or(POLL, |wait| wait.min(POLL)))
            } else {
                until_save
            };
            let message = match wait {
                Some(wait) => messages.recv_timeout(wait),
                None => messages.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match message {
                Ok(message) => self.handle(message),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if watching {
                reveal();
            }
            if self.save_at.is_some_and(|at| Instant::now() >= at) {
                self.save();
            }
        }
    }

    fn handle(&mut self, message: Message) {
        match message {
            Message::Changed(label) => {
                let changed = with_live(&label, |live| {
                    let rect = (live.raw != 0)
                        .then(|| platform::window_rect(live.raw))
                        .flatten()?;
                    let collapsed = live.collapse.get();
                    let note = live.kind == Kind::Note;
                    let dirty = Dirty {
                        rect: floats::saved_rect(rect, collapsed, live.expanded_height),
                        opacity: live.opacity,
                        text_scale: note.then_some(live.text_scale),
                        collapsed: note.then_some(collapsed),
                        folder: live.folder.clone(),
                        epoch: live.epoch,
                    };
                    Some((live.id.clone(), dirty))
                });
                if let Ok(Some((id, dirty))) = changed {
                    self.dirty.insert(id, dirty);
                    self.save_at = Some(Instant::now() + SAVE_DELAY);
                }
            }
            Message::Wake => {}
            Message::Close(label) => close(&self.app, &label),
            Message::Destroyed(label) => {
                take(&label);
                report(&self.app);
            }
            Message::UnlockAll => unlock(&self.app),
            Message::CloseAll => close_all(&self.app),
            Message::Flush(done) => {
                self.save();
                let _ = done.send(());
            }
            Message::NotesChanged(root, renames) => {
                notes_changed(&self.app, root.as_deref(), &renames);
            }
        }
    }

    /// Saves the moves and fades waiting, each only into the folder its window opened on, and only
    /// while that folder is still the one configured and its epoch lasts, checked under `SAVING`;
    /// a window closed meanwhile changes nothing.
    fn save(&mut self) {
        self.save_at = None;
        let mut by_folder: Vec<(FolderIdentity, Vec<(String, Dirty)>)> = Vec::new();
        for (id, dirty) in std::mem::take(&mut self.dirty) {
            match by_folder
                .iter_mut()
                .find(|(folder, _)| *folder == dirty.folder)
            {
                Some((_, changes)) => changes.push((id, dirty)),
                None => by_folder.push((dirty.folder.clone(), vec![(id, dirty)])),
            }
        }
        for (folder, changes) in by_folder {
            let saved = crate::notes::update_floating(&self.app, &folder, |store| {
                let epoch = registry().epoch;
                let current: Vec<_> = changes.iter().filter(|(_, d)| d.epoch == epoch).collect();
                if current.is_empty() {
                    return Err(Error::FolderChanged);
                }
                for (id, dirty) in current {
                    store.update(
                        id,
                        dirty.rect,
                        dirty.opacity,
                        dirty.text_scale,
                        dirty.collapsed,
                    );
                }
                Ok(())
            });
            if let Err(error) = saved {
                eprintln!("[floating] position not saved: {error:?}");
            }
        }
    }
}

/// Lets a click-through window take clicks while Alt is held over it (`floats::reveal_step`), and
/// lets them through again once Alt and any drag it started are released.
fn reveal() {
    let alt = platform::alt_held();
    let button = platform::primary_button_down();
    let cursor = platform::cursor_pos();
    let mut registry = registry();
    for live in registry.live.iter_mut() {
        if !live.click_through || live.raw == 0 {
            continue;
        }
        let over = cursor
            .zip(platform::window_rect(live.raw))
            .is_some_and(|(cursor, rect)| rect.contains(cursor));
        let revealed = floats::reveal_step(live.revealed, alt, over, button);
        if revealed != live.revealed {
            live.revealed = revealed;
            platform::set_click_through(live.raw, !revealed);
            live.changed();
        }
    }
}

/// Sends Escape through the post-it's state channel without taking keyboard focus.
pub fn escape_over_post_its(cursor: Point) {
    for live in &mut registry().live {
        if live.kind == Kind::Note
            && live.shown
            && (!live.click_through || live.revealed)
            && platform::window_rect(live.raw).is_some_and(|rect| rect.contains(cursor))
        {
            live.escape_revision += 1;
            live.changed();
        }
    }
}

/// The tray's "Unlock floating windows" is available while any window lets clicks through.
/// Tells the tray and `main` what changed: "Unlock floating windows" is available while any
/// window lets clicks through and "Close all floating windows" while any is open, and `main` marks
/// the images floating windows show (`floating-paths`).
fn report(app: &AppHandle) {
    // One report at a time, each reading the registry inside: `main` and the tray never end on an
    // older state than the last change. Never taken on the main thread nor under the registry.
    let _reporting = REPORTING.lock().unwrap_or_else(|e| e.into_inner());
    let (through, open, paths, editing) = {
        let registry = registry();
        let through = registry.live.iter().any(|live| live.click_through);
        let open = !registry.live.is_empty();
        (through, open, registry.paths(), registry.editing_paths())
    };
    crate::tray::set_floating_items(app, through, open);
    if let Err(error) = app.emit_to("main", PATHS, paths) {
        eprintln!("[floating] open images not reported: {error}");
    }
    if let Err(error) = app.emit_to("main", EDITING, editing) {
        eprintln!("[floating] edited notes not reported: {error}");
    }
}

/// Tells `main` which images floating windows show.
const PATHS: &str = "floating-paths";
/// Tells `main` which notes post-its edit.
const EDITING: &str = "floating-editing";

/// The images and notes the open floating windows show, relative to the notes folder.
pub fn paths() -> Vec<String> {
    registry().paths()
}

fn unlock(app: &AppHandle) {
    {
        let mut registry = registry();
        for live in registry.live.iter_mut().filter(|live| live.click_through) {
            live.click_through = false;
            live.revealed = false;
            if live.raw != 0 {
                platform::set_click_through(live.raw, false);
            }
            live.changed();
        }
    }
    report(app);
}

/// Starts the worker; floating windows are opened and restored afterwards.
pub fn start(app: &AppHandle) {
    let (sender, messages) = mpsc::channel();
    if WORKER.set(sender).is_err() {
        return;
    }
    let worker = Worker {
        app: app.clone(),
        dirty: BTreeMap::new(),
        save_at: None,
    };
    thread::spawn(move || worker.run(messages));
}

fn new_id() -> String {
    format!(
        "{:016x}{:016x}",
        crate::trash::random(),
        crate::trash::random()
    )
}

/// `Registry::reserve` on the app's registry, registering the label as live.
fn reserve(epoch: u64, entry: &Entry, folder: &FolderIdentity) -> Result<(String, usize), Error> {
    let mut registry = registry();
    let reserved = registry.reserve(epoch, entry, folder)?;
    admit(&reserved.0);
    Ok(reserved)
}

/// Removes the window `label` from the registry and the live labels, if it is still there.
fn take(label: &str) -> Option<Live> {
    forget_label(label);
    registry().take(label)
}

fn destroy(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label)
        && let Err(error) = window.destroy()
    {
        eprintln!("[floating] window not destroyed: {error}");
    }
}

/// Builds the reserved window `label` hidden, hardened as every secondary app window, as a tool
/// window that never activates. Gives up, destroying it, if the registry no longer holds it, as
/// after a close or the notes folder going away.
fn build(app: &AppHandle, label: &str, kind: Kind, aspect: f64) -> Result<(), Error> {
    let side = f64::from(floats::MIN_SIDE);
    // A post-it collapses to its title bar, so its window may get that short.
    let least_height = match kind {
        Kind::Image => side,
        Kind::Note => f64::from(floats::TITLE_HEIGHT + 2 * floats::SHADOW_PAD),
    };
    let collapse = with_live(label, |live| live.collapse.clone())?;
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("DocDock")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .focused(false)
        .resizable(true)
        .min_inner_size(side, least_height);
    let window = crate::main_window::secondary_window(app, builder)?;
    let raw = match window.hwnd() {
        Ok(hwnd) => hwnd.0 as isize,
        Err(error) => {
            let _ = window.destroy();
            return Err(Error::unexpected(error));
        }
    };
    platform::prepare_floating(raw);
    // Resizing keeps an image's proportions, so the window never shows margins around it; a
    // post-it takes any.
    let aspect = Aspect::new(aspect);
    let hooked = aspect.clone();
    let sized = window.run_on_main_thread(move || {
        let sizing: platform::Sizing = Box::new(move |rect, edge, dpi, work| {
            let pad = scale_dpi(dpi, floats::SHADOW_PAD);
            let min = scale_dpi(dpi, floats::MIN_SIDE);
            match kind {
                Kind::Image => floats::constrain_sizing(rect, edge, hooked.get(), pad, min, work),
                Kind::Note => {
                    let bar = collapse.get().then(|| floats::collapsed_height(dpi, pad));
                    floats::constrain_free(rect, edge, pad, min, work, bar)
                }
            }
        });
        if let Err(error) = platform::constrain_sizing(raw, sizing) {
            eprintln!("[floating] resizing keeps no proportions: {error}");
        }
    });
    if let Err(error) = sized {
        eprintln!("[floating] resizing keeps no proportions: {error}");
    }
    if with_live(label, |live| {
        live.raw = raw;
        live.aspect = aspect;
    })
    .is_err()
    {
        let _ = window.destroy();
        return Err(Error::unexpected("floating window closed while opening"));
    }
    Ok(())
}

/// Shows the hidden window `raw` at `rect`, above other windows, without activating it. Twice: a
/// window landing on a monitor of another scale is resized by the DPI change.
fn show(raw: isize, rect: Rect) {
    platform::show_topmost(raw, rect);
    platform::show_topmost(raw, rect);
}

/// Shows the built window `label` only if no close or folder change came since the opening
/// started on `epoch`. Under the registry's lock, so neither can slip in between.
fn show_if_current(
    registry: &Mutex<Registry>,
    label: &str,
    epoch: u64,
    show: impl FnOnce(isize, f64),
) -> bool {
    let mut registry = lock(registry);
    let Some((raw, aspect)) = registry.confirm(label, epoch) else {
        return false;
    };
    show(raw, aspect);
    if let Some(live) = registry.live.iter_mut().find(|live| live.label == label) {
        live.shown = true;
    }
    true
}

/// The end of opening a new window, built hidden: `save` its entry, which must ask `admit` under
/// `SAVING`, then show it only if neither a close nor a folder change came meanwhile. A close or a
/// folder change before `admit` leaves nothing saved; a close after it forgets the entry itself;
/// a folder change after it destroys the window and keeps the entry, rightly saved in the folder
/// it opened on. So nothing needs rolling back across folders.
fn save_and_show(
    registry: &Mutex<Registry>,
    label: &str,
    epoch: u64,
    save: impl FnOnce(&mut dyn FnMut() -> Result<(), Error>) -> Result<(), Error>,
    show: impl FnOnce(isize, f64),
) -> Result<(), Error> {
    save(&mut || lock(registry).admits(label, epoch))?;
    if show_if_current(registry, label, epoch, show) {
        Ok(())
    } else {
        Err(Error::FolderChanged)
    }
}

/// Detaches `path`, an image relative to the notes folder, into a floating window where the side
/// preview would show it for its thumbnail at `thumb` (a CSS box in `main`), slightly apart from
/// the windows already open. Its entry is saved before it shows.
pub fn open(
    app: &AppHandle,
    requests: &Sender<Request>,
    path: String,
    thumb: [i32; 4],
    natural: (u32, u32),
) -> Result<(), Error> {
    open_kind(app, requests, Kind::Image, path, thumb, natural)
}

/// Opens the note `path`, relative to the notes folder, as a post-it beside its card at `card` (a
/// CSS box in `main`), at `floats::NOTE_SIZE`, slightly apart from the windows already open. Its
/// entry is saved before it shows.
pub fn open_note(
    app: &AppHandle,
    requests: &Sender<Request>,
    path: String,
    card: [i32; 4],
) -> Result<(), Error> {
    open_kind(app, requests, Kind::Note, path, card, floats::NOTE_SIZE)
}

/// Opens a window of `kind` for `path` beside `from`, a CSS box in `main`, where the side preview
/// would show something of `natural` pixels.
fn open_kind(
    app: &AppHandle,
    requests: &Sender<Request>,
    kind: Kind,
    path: String,
    from: [i32; 4],
    natural: (u32, u32),
) -> Result<(), Error> {
    open_with_rect(
        app,
        kind,
        path,
        None,
        |_| {},
        |open| {
            let placed = place(requests, from, natural)?;
            let aspect = match natural {
                (width, height) if width > 0 && height > 0 => f64::from(width) / f64::from(height),
                _ => floats::aspect_of(placed.image),
            };
            let pad = scale_dpi(placed.dpi, floats::SHADOW_PAD);
            let window = floats::outset(placed.image, pad);
            let window = floats::cascade(window, open, placed.dpi, placed.side, placed.free);
            let rect = fitted(window, kind, aspect, &platform::monitors());
            Ok((aspect, rect))
        },
    )
}

fn open_with_rect(
    app: &AppHandle,
    kind: Kind,
    path: String,
    origin: Option<(u64, FolderIdentity)>,
    on_reserved: impl FnOnce(&str),
    compute: impl FnOnce(usize) -> Result<(f64, Rect), Error>,
) -> Result<(), Error> {
    let (epoch, folder) = match origin {
        Some((epoch, folder)) => {
            if !registry().current(epoch) {
                return Err(Error::FolderChanged);
            }
            let current = crate::notes::floating_folder(app, kind, &path)?;
            if current != folder {
                return Err(Error::FolderChanged);
            }
            (epoch, folder)
        }
        None => (
            registry().epoch,
            crate::notes::floating_folder(app, kind, &path)?,
        ),
    };
    let mut entry = Entry {
        id: new_id(),
        kind,
        path,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        opacity: 1.0,
        text_scale: None,
        collapsed: false,
    };
    let (label, open) = reserve(epoch, &entry, &folder)?;
    on_reserved(&label);
    let opened = compute(open).and_then(|(aspect, rect)| {
        entry.set_rect(rect);
        build(app, &label, kind, aspect)?;
        save_and_show(
            &REGISTRY,
            &label,
            epoch,
            |admit| crate::notes::add_floating(app, &folder, entry, admit),
            |raw, _| show(raw, rect),
        )
    });
    if opened.is_err() {
        take(&label);
        destroy(app, &label);
    }
    report(app);
    opened
}

/// Starts a pointer-capture drag for `path`, targeting an existing post-it window or a live ghost.
pub fn start_drag_note(
    app: &AppHandle,
    requests: &Sender<Request>,
    id: u64,
    path: String,
) -> Result<(), Error> {
    let (epoch, existing) = {
        let registry = registry();
        if registry.switching > 0 {
            return Err(Error::FolderChanged);
        }
        let existing = registry
            .live
            .iter()
            .find(|live| {
                live.kind == Kind::Note
                    && !live.closing
                    && !live.parked
                    && live.raw != 0
                    && same(&live.path, &path)
            })
            .map(|live| (live.label.clone(), live.raw));
        if existing.is_none() && registry.live.len() >= MAX_FLOATING {
            return Err(Error::FloatingLimit { max: MAX_FLOATING });
        }
        (registry.epoch, existing)
    };
    let target = if let Some((label, raw)) = existing
        && let Some(origin) = platform::window_rect(raw)
    {
        crate::dock::NoteDragTarget::Existing { label, raw, origin }
    } else {
        let (generation, folder) = crate::preview::begin_post_it(app, requests, path.clone())?;
        if !registry().current(epoch) {
            crate::preview::close(app);
            return Err(Error::FolderChanged);
        }
        crate::dock::NoteDragTarget::Ghost {
            path,
            generation,
            epoch,
            folder,
        }
    };
    requests
        .send(Request::StartNoteDrag { id, target })
        .map_err(Error::unexpected)
}

/// Finishes the active note drag and opens or confirms the moved post-it when committed outside.
pub fn end_drag_note(
    app: &AppHandle,
    requests: &Sender<Request>,
    id: u64,
    commit: bool,
    cursor: Option<[i32; 2]>,
) -> Result<bool, Error> {
    let (reply, outcome) = mpsc::channel();
    requests
        .send(Request::EndNoteDrag {
            id,
            commit,
            cursor,
            reply,
        })
        .map_err(Error::unexpected)?;
    match outcome
        .recv_timeout(PLACE_WAIT)
        .map_err(Error::unexpected)?
    {
        crate::dock::EndDragOutcome::Cancelled => Ok(false),
        crate::dock::EndDragOutcome::Moved { label } => {
            send(Message::Changed(label));
            Ok(true)
        }
        crate::dock::EndDragOutcome::Spawn {
            path,
            rect,
            generation,
            epoch,
            folder,
        } => {
            let rect = fitted(rect, Kind::Note, 1.0, &platform::monitors());
            let opened = open_with_rect(
                app,
                Kind::Note,
                path,
                Some((epoch, folder)),
                |label| {
                    let _ = requests.send(Request::BindDragHandoff {
                        generation,
                        label: label.to_owned(),
                    });
                },
                |_| Ok((1.0, rect)),
            );
            if let Err(error) = opened {
                let _ = requests.send(Request::FinishDragHandoff(
                    crate::dock::DragHandoffEnd::Generation(generation),
                ));
                if matches!(error, Error::FolderChanged) {
                    return Ok(false);
                }
                return Err(error);
            }
            Ok(true)
        }
    }
}

/// Dismisses the drag ghost waiting on `label` once that newly opened post-it renders its frame.
pub fn ready(label: &str, requests: &Sender<Request>) {
    let _ = requests.send(Request::FinishDragHandoff(
        crate::dock::DragHandoffEnd::Label(label.to_owned()),
    ));
}

/// `window` on the monitors there are now and, for an image of `aspect`, reshaped to it.
fn fitted(window: Rect, kind: Kind, aspect: f64, monitors: &[platform::Monitor]) -> Rect {
    let rect = floats::fit_to_monitors(window, monitors);
    if kind == Kind::Note {
        return rect;
    }
    let dpi = floats::monitor_of(rect, monitors).map_or(platform::BASE_DPI, |m| m.dpi);
    floats::contain(rect, aspect, scale_dpi(dpi, floats::SHADOW_PAD))
}

/// Where the panel thread places a window for the thumbnail.
fn place(
    requests: &Sender<Request>,
    thumb: [i32; 4],
    natural: (u32, u32),
) -> Result<floats::Placed, Error> {
    let (reply, placed) = mpsc::channel();
    requests
        .send(Request::PlaceFloat {
            rect: thumb,
            natural,
            reply,
        })
        .map_err(Error::unexpected)?;
    placed.recv_timeout(PLACE_WAIT).map_err(Error::unexpected)
}

/// Opens a window for the saved `entry` at `rect`, for work started on `epoch`; a collapsed
/// post-it's `rect` is its expanded rectangle.
fn open_saved(
    app: &AppHandle,
    entry: &Entry,
    rect: Rect,
    epoch: u64,
    folder: &FolderIdentity,
) -> Result<(), Error> {
    // A collapsed post-it keeps the expanded height it fitted to.
    let mut entry = entry.clone();
    entry.set_rect(rect);
    let (label, _) = reserve(epoch, &entry, folder)?;
    let dpi = floats::monitor_of(rect, &platform::monitors()).map_or(platform::BASE_DPI, |m| m.dpi);
    let aspect = floats::aspect_of(floats::inset(rect, scale_dpi(dpi, floats::SHADOW_PAD)));
    let pad = scale_dpi(dpi, floats::SHADOW_PAD);
    let kind = entry.kind;
    let collapsed = kind == Kind::Note && entry.collapsed;
    // The image may have loaded and told its real proportions (`natural`) while hidden.
    let shown = build(app, &label, kind, aspect).and_then(|()| {
        let fit = |raw, aspect| match kind {
            Kind::Image => show(raw, floats::contain(rect, aspect, pad)),
            Kind::Note if collapsed => show(raw, floats::collapsed_rect(rect, dpi, pad)),
            Kind::Note => show(raw, rect),
        };
        show_if_current(&REGISTRY, &label, epoch, fit)
            .then_some(())
            .ok_or(Error::FolderChanged)
    });
    if shown.is_err() {
        take(&label);
        destroy(app, &label);
    }
    shown
}

/// Reopens the windows saved in the notes folder, unlocked, where they were as far as the
/// monitors there are now allow. Entries whose image or note is gone are dropped; those that cannot
/// be checked now stay for the next time. Stops once the folder leaves or another is chosen. Runs
/// off the main thread.
pub fn restore(app: &AppHandle) {
    let _restoring = RESTORING.lock().unwrap_or_else(|e| e.into_inner());
    let epoch = {
        let registry = registry();
        if registry.switching > 0 {
            return;
        }
        registry.epoch
    };
    let (folder, store) = match crate::notes::read_floating(app) {
        Ok(read) => read,
        Err(error) => {
            eprintln!("[floating] saved windows not read: {error:?}");
            return;
        }
    };
    {
        let mut registry = registry();
        if !registry.current(epoch) {
            return;
        }
        registry.adopt(&folder);
        registry.refresh_blocked();
    }
    let Some(store) = store else {
        report(app);
        return;
    };
    let monitors = platform::monitors();
    let mut gone = Vec::new();
    for entry in store.entries() {
        let open = {
            let registry = registry();
            if !registry.current(epoch) {
                return;
            }
            // A post-it parked from this folder and just adopted is open already.
            registry
                .live
                .iter()
                .any(|live| live.id == entry.id && live.folder == folder)
        };
        if open {
            continue;
        }
        match crate::notes::floating_present(app, &folder, entry.kind, &entry.path) {
            Ok(true) => {}
            Ok(false) => {
                gone.push(entry.id.as_str());
                continue;
            }
            Err(_) => continue,
        }
        let saved = entry.rect();
        let dpi = floats::monitor_of(saved, &monitors).map_or(platform::BASE_DPI, |m| m.dpi);
        let aspect = floats::aspect_of(floats::inset(saved, scale_dpi(dpi, floats::SHADOW_PAD)));
        let rect = if entry.kind == Kind::Note && entry.collapsed {
            floats::fit_collapsed(saved, &monitors)
        } else {
            fitted(saved, entry.kind, aspect, &monitors)
        };
        match open_saved(app, entry, rect, epoch, &folder) {
            Ok(()) => {}
            Err(Error::FloatingLimit { .. } | Error::FolderChanged) => break,
            Err(error) => eprintln!("[floating] window not reopened: {error:?}"),
        }
    }
    report(app);
    if !gone.is_empty()
        && let Err(error) = crate::notes::update_floating(app, &folder, |store| {
            if !registry().current(epoch) {
                return Err(Error::FolderChanged);
            }
            store.remove(&gone);
            Ok(())
        })
    {
        eprintln!("[floating] gone images and notes not dropped: {error:?}");
    }
}

/// The notes folder went away, such as a vault that locked, or another is about to be chosen: a
/// new epoch starts, every window stops reaching its commands and images at once, then goes; its
/// saved entry stays for when the folder returns
/// ([0009](../../docs/decisions/0009-note-privacy.md)). Openings still on their way give up. A
/// post-it editing its note is parked instead (`Registry::revoke`): it hides its text, and its
/// commands reach the note and its images only in the folder it opened on, by identity.
pub fn folder_left(app: &AppHandle) {
    revoke_all(app, Registry::revoke);
}

/// Revokes every window and destroys it; first forgets the entries of those the user was closing,
/// while their folder is still the one configured, so a close overtaken by a folder change never
/// comes back with that folder.
fn revoke_all(app: &AppHandle, revoke: impl FnOnce(&mut Registry) -> Vec<Live>) {
    let gone = {
        let mut registry = registry();
        let gone = revoke(&mut registry);
        for live in &gone {
            forget_label(&live.label);
        }
        gone
    };
    for (folder, ids) in closing(&gone) {
        let forgot = crate::notes::update_floating(app, folder, |store| {
            store.remove(&ids);
            Ok(())
        });
        if let Err(error) = forgot {
            eprintln!("[floating] closed window not forgotten: {error:?}");
        }
    }
    for live in &gone {
        destroy(app, &live.label);
    }
    report(app);
}

/// The ids of the windows in `gone` the user was closing, by folder.
fn closing(gone: &[Live]) -> Vec<(&FolderIdentity, Vec<&str>)> {
    let mut by_folder: Vec<(&FolderIdentity, Vec<&str>)> = Vec::new();
    for live in gone.iter().filter(|live| live.closing) {
        match by_folder
            .iter_mut()
            .find(|(folder, _)| **folder == live.folder)
        {
            Some((_, ids)) => ids.push(&live.id),
            None => by_folder.push((&live.folder, vec![&live.id])),
        }
    }
    by_folder
}

/// The notes folder came back: its windows reopen.
pub fn folder_returned(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || restore(&app));
}

/// A folder choice under way (`begin_switch`); dropping it ends the choice and reopens the
/// configured folder's windows.
pub struct SwitchGuard(AppHandle);

impl Drop for SwitchGuard {
    fn drop(&mut self) {
        let ended = {
            let mut registry = registry();
            // The edits `finish_edits` held for the choice may begin again.
            registry.held = false;
            registry.end_switch()
        };
        if ended {
            folder_returned(&self.0);
        }
    }
}

/// Another notes folder is about to be chosen, or the one at the path trusted: as `folder_left`,
/// and no window opens or reopens until the returned guard drops, after the settings change, with
/// a new epoch again.
pub fn begin_switch(app: &AppHandle) -> SwitchGuard {
    revoke_all(app, Registry::begin_switch);
    SwitchGuard(app.clone())
}

/// The notes folder the live floating window `label` opened on, for `note-asset`.
pub fn folder_of(label: &str) -> Option<FolderIdentity> {
    registry()
        .live
        .iter()
        .find(|live| live.label == label)
        .map(|live| live.folder.clone())
}

/// The user closed `label`: its saved entry goes first, then its window. The window is marked
/// closing before anything else, so a folder change that revokes it meanwhile forgets the entry
/// itself (`revoke_all`) while its folder is still configured; the folder identity check keeps the
/// removal from ever reaching another folder.
pub fn close(app: &AppHandle, label: &str) {
    let closing = registry().mark_closing_where(|live| live.label == label);
    forget_closed(app, closing);
}

/// The post-it or image `label` closes itself, from its own close button: a post-it does so only
/// once nothing is left to save (or the user discarded it), so its edit ends first, giving focus
/// back. A parked post-it then goes keeping its entry, which its folder, gone, cannot forget.
pub fn close_own(app: &AppHandle, label: &str) -> Result<(), Error> {
    if with_live(label, |live| live.editing)? {
        end_edit(app, label)?;
    }
    close(app, label);
    Ok(())
}

/// The user closes every window showing `path`, an image or a note however it is spelled, as
/// `close` does.
pub fn close_path(app: &AppHandle, path: &str) {
    let same = same_path(path);
    let closing = registry().mark_closing_where(|live| !live.parked && same(live));
    forget_closed(app, closing);
}

/// The user closes every window, as `close` does. Entries of other kinds stay.
pub fn close_all(app: &AppHandle) {
    let closing = registry().mark_closing_where(|_| true);
    forget_closed(app, closing);
}

/// Picks the windows showing `path`, compared as saved image paths are (`attachments::normalize`).
fn same_path(path: &str) -> impl Fn(&Live) -> bool {
    move |live| same(&live.path, path)
}

/// Two paths relative to the notes folder name the same file, as `attachments::normalize` compares
/// them: either separator, `.` and `..`, and case.
fn same(a: &str, b: &str) -> bool {
    let a = crate::attachments::normalize(a);
    a.is_some() && a == crate::attachments::normalize(b)
}

/// Forgets the entries of the windows the user closed, each in its own folder while it is still
/// the one configured, then removes and destroys them.
fn forget_closed(app: &AppHandle, closing: Vec<Closing>) {
    if closing.is_empty() {
        return;
    }
    let mut by_folder: Vec<(&FolderIdentity, Vec<&str>)> = Vec::new();
    for closed in &closing {
        match by_folder
            .iter_mut()
            .find(|(folder, _)| **folder == closed.folder)
        {
            Some((_, ids)) => ids.push(&closed.id),
            None => by_folder.push((&closed.folder, vec![&closed.id])),
        }
    }
    for (folder, ids) in by_folder {
        let forgot = crate::notes::update_floating(app, folder, |store| {
            store.remove(&ids);
            Ok(())
        });
        if let Err(error) = forgot {
            eprintln!("[floating] closed window not forgotten: {error:?}");
        }
    }
    for closed in &closing {
        take(&closed.label);
        destroy(app, &closed.label);
    }
    report(app);
}

/// From the tray: every window closes. Safe on the main thread.
pub fn close_all_requested() {
    send(Message::CloseAll);
}

/// The window's state when it loads; later changes go to `channel`, replacing the one it handed
/// over before a reload.
pub fn state(label: &str, channel: Channel<Update>, locale: Locale) -> Result<State, Error> {
    with_live(label, |live| {
        live.channel = Some(channel);
        State {
            update: live.update(),
            locale,
        }
    })
}

/// The note the post-it `label` shows, with its version, read now from the folder it opened on;
/// `None` once the note is gone.
pub fn note(app: &AppHandle, label: &str) -> Result<Option<OpenNote>, Error> {
    let (kind, path, folder) = with_live(label, |live| {
        (live.kind, live.path.clone(), live.folder.clone())
    })?;
    if kind != Kind::Note {
        return Err(Error::unexpected("command restricted to post-its"));
    }
    let text = crate::notes::floating_note(app, &folder, &path)?;
    // A rename or folder change while reading: what was read belongs to no window any more.
    let still = with_live(label, |live| live.path == path && live.folder == folder)?;
    if !still {
        return Err(Error::FolderChanged);
    }
    Ok(text)
}

/// Sizes the post-it's text against the panel's.
pub fn set_text_scale(label: &str, scale: f64) -> Result<(), Error> {
    with_live(label, |live| {
        if live.kind != Kind::Note {
            return Err(Error::unexpected("command restricted to post-its"));
        }
        live.text_scale = floats::clamp_text_scale(scale);
        live.changed();
        Ok(())
    })??;
    send(Message::Changed(label.to_owned()));
    Ok(())
}

/// Collapses the shown post-it `label` to its title bar, keeping its top, or expands it back to
/// its height before, within its monitor's work area. One at a time, so two quick toggles never
/// take the bar's height for the expanded one; Win32 calls run outside the registry's lock, and the
/// state changes so a save in between still records the expanded height (`floats::saved_rect`).
pub fn set_collapsed(label: &str, collapsed: bool) -> Result<(), Error> {
    let _collapsing = COLLAPSING.lock().unwrap_or_else(|e| e.into_inner());
    let (kind, collapse, expanded_height) = with_live(label, |live| {
        (live.kind, live.collapse.clone(), live.expanded_height)
    })?;
    if kind != Kind::Note {
        return Err(Error::unexpected("command restricted to post-its"));
    }
    let (raw, _) = shown(label)?;
    if collapse.get() == collapsed {
        return Ok(());
    }
    let (window, monitor, pad, _) = measured(raw)?;
    if collapsed {
        with_live(label, |live| {
            live.expanded_height = window.bottom - window.top;
            live.collapse.set(true);
        })?;
        platform::move_window(raw, floats::collapsed_rect(window, monitor.dpi, pad));
    } else {
        platform::move_window(
            raw,
            floats::expand_rect(window, expanded_height, monitor.work),
        );
        with_live(label, |live| live.collapse.set(false))?;
    }
    with_live(label, Live::changed)?;
    send(Message::Changed(label.to_owned()));
    Ok(())
}

/// Opens the post-it's note in the panel for editing, while the notes folder it opened on lasts.
pub fn edit(label: &str, requests: &Sender<Request>) -> Result<(), Error> {
    let (kind, path, epoch) = with_live(label, |live| (live.kind, live.path.clone(), live.epoch))?;
    if kind != Kind::Note {
        return Err(Error::unexpected("command restricted to post-its"));
    }
    requests
        .send(Request::EditNote(EditNoteRequest { path, epoch }))
        .map_err(Error::unexpected)
}

/// A post-it's request to edit, made on `epoch`, still names a note of the configured folder: no
/// folder change came since, nor is one under way. The panel asks before opening it, since the
/// request may reach it after another folder was chosen.
pub fn edit_current(epoch: u64) -> bool {
    registry().current(epoch)
}

/// The post-it `label` begins editing its note, which nobody else edits (`Registry::may_edit`),
/// and takes focus; a note edited elsewhere is saved there first (`Transfer`), so this waits and
/// never runs on the main thread. Answers the note with its version and the edit granted, which
/// `save_note` names; `None` if it may not edit now, as while edits are held, the editor holding
/// its note could not save, it lets clicks through or the note is gone.
pub fn begin_edit(app: &AppHandle, label: &str) -> Result<Option<Granted>, Error> {
    // Asking to edit a note edited elsewhere moves the edit here: the editor holding it saves and
    // returns to reading first. Before `LEASING`, which its answer takes to end its edit.
    let transfer = registry().begin_transfer(label)?;
    match transfer {
        Transfer::Refuse => return Ok(None),
        Transfer::Finish(holder) if !finish_editor(app, &holder) => return Ok(None),
        Transfer::Finish(_) | Transfer::Grant => {}
    }
    let _leasing = LEASING.lock().unwrap_or_else(|e| e.into_inner());
    // Claimed before reading, so nobody else begins editing the note in between.
    let Some(claim) = registry().begin_edit(label)? else {
        return Ok(None);
    };
    // The claim still stands: no rename, folder change, close or other edit came meanwhile.
    let kept = |live: &Live| {
        live.editing
            && !live.closing
            && live.lease == claim.lease
            && live.path == claim.path
            && live.folder == claim.folder
    };
    let read = crate::notes::floating_note(app, &claim.folder, &claim.path);
    let raw = match &read {
        Ok(Some(_)) => {
            let foreground = platform::foreground_window();
            // What was read belongs to no window any more if the claim fell.
            with_live(label, |live| {
                let kept = kept(live);
                if kept && live.previous_focus.is_none() {
                    live.previous_focus = foreground.filter(|&raw| raw != live.raw);
                }
                kept.then_some(live.raw)
            })
            .ok()
            .flatten()
        }
        _ => None,
    };
    let Some(raw) = raw else {
        let _ = leave(app, label);
        return read.map(|_| None);
    };
    platform::set_activatable(raw, true);
    // A failed `SetForegroundWindow` still grants the edit: the window no longer refuses
    // activation, so the user's next click in it activates it natively.
    platform::activate(raw);
    // A close or folder change between reading the window and activating it: no edit stays
    // granted on a window on its way out, and it never activates again.
    if !with_live(label, |live| kept(live)).unwrap_or(false) {
        platform::set_activatable(raw, false);
        let same_edit = registry()
            .live
            .iter()
            .any(|live| live.label == label && live.lease == claim.lease);
        if same_edit {
            let _ = leave(app, label);
        }
        return Ok(None);
    }
    report(app);
    read.map(|note| {
        note.map(|note| Granted {
            note,
            lease: claim.lease,
        })
    })
}

/// Saves `content` as the note the post-it `label` edits, if it still holds `base` and the edit
/// `lease` it was granted lasts. Never names a path: only the note Rust registered for the window,
/// read under `SAVING` so a rename, close or end of the edit in between is seen, and only in the
/// notes folder it opened on. A missing note answers `Conflict` unless `recreate`.
pub fn save_note(
    app: &AppHandle,
    label: &str,
    content: &str,
    base: &str,
    lease: u64,
    recreate: bool,
) -> Result<crate::notes::Saved, Error> {
    let (_, folder) = registry().editing(label, lease)?;
    crate::notes::floating_save(app, &folder, content, base, recreate, || {
        registry().editing(label, lease).map(|(path, _)| path)
    })
}

/// Whether the post-it `label` may write the clipboard now (`Registry::copies`): its editor's
/// copies and cuts go through Rust, as the panel's do, kept out of clipboard history.
pub fn copies(label: &str) -> Result<(), Error> {
    registry().copies(label)
}

/// The post-it `label` stops editing and never activates again; if it still has focus, focus goes
/// back to the window that had it before, or the one the panel last opened over, never to a
/// window of the app. If the user went to another window meanwhile, focus stays there. A parked
/// post-it, whose folder went while it edited, goes as a revoked window does, its entry kept: the
/// UI ends its edit once nothing is left to save, or when the user discards what was.
pub fn end_edit(app: &AppHandle, label: &str) -> Result<(), Error> {
    let _leasing = LEASING.lock().unwrap_or_else(|e| e.into_inner());
    leave(app, label)
}

/// `end_edit` under `LEASING`.
fn leave(app: &AppHandle, label: &str) -> Result<(), Error> {
    let Some(Ended {
        raw,
        previous_focus: previous,
        parked,
    }) = registry().end_edit(label)?
    else {
        return Ok(());
    };
    if raw != 0 {
        platform::set_activatable(raw, false);
        if platform::is_foreground(raw) {
            let outside = |raw: &isize| !platform::is_own_window(*raw);
            let target = previous
                .filter(outside)
                .or_else(|| crate::dock::previous_window().filter(outside));
            platform::restore_focus(target);
        }
    }
    if parked {
        take(label);
        destroy(app, label);
    }
    report(app);
    Ok(())
}

/// The panel edits the note `path`, relative to the notes folder, or none (`None`). A post-it
/// editing it is asked to finish first, so the edit moves to the panel (`transfer`). Answers
/// whether it may. Waits, so never on the main thread.
pub fn panel_edit(app: &AppHandle, path: Option<String>) -> bool {
    if let Some(path) = &path {
        let transfer = registry().transfer(&Editor::Panel, path);
        match transfer {
            Transfer::Refuse => return false,
            Transfer::Finish(holder) if !finish_editor(app, &holder) => return false,
            Transfer::Finish(_) | Transfer::Grant => {}
        }
    }
    registry().panel_edit(path)
}

/// Asks `editor` to finish its edit and waits for it (`finish_where`, `release_panel`).
fn finish_editor(app: &AppHandle, editor: &Editor) -> bool {
    match editor {
        Editor::Panel => release_panel(app),
        Editor::Float(label) => finish_where(false, |live| live.label == *label),
    }
}

// Finishing: Rust asks editors to save and leave their edit, and waits for their answers, failing
// closed. Used by the exit gate and folder choices (`finish_edits`, every post-it) and by edit
// transfers (`finish_editor`, the one editor holding the note). A request raises
// `Update::finish` on each post-it asked, which answers with `float_finished`; the panel hears
// `floating-release-panel-edit` and answers with `floating_panel_released`. Each request waits in
// `FINISHES` until all answered, one failed or was parked, or `FLUSH_WAIT` passed. Never waited on
// under the registry, nor on the main thread: the answers come through commands that take the
// registry and `LEASING`.

/// Where an editor asked to finish stands, as the registry sees it while the request waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Standing {
    Editing,
    /// Its folder went: it cannot save now.
    Parked,
    /// It stopped editing, or went: nothing of it is left to wait for.
    Done,
}

/// A request for editors to finish their edits.
#[derive(Debug)]
struct Finishing {
    request: u64,
    /// The editors still to answer: post-its by label, the panel as `PANEL`.
    pending: BTreeSet<String>,
    failed: bool,
    deadline: Instant,
}

impl Finishing {
    fn new(request: u64, editors: Vec<(String, Standing)>, deadline: Instant) -> Self {
        let mut finishing = Self {
            request,
            pending: editors.iter().map(|(key, _)| key.clone()).collect(),
            failed: false,
            deadline,
        };
        for (key, standing) in editors {
            finishing.stand(&key, standing);
        }
        finishing
    }

    /// The editor `key` answered request `request`. An answer to an earlier request is stale and
    /// ignored; one to a later request covers this one, since it finished later.
    fn answer(&mut self, request: u64, key: &str, saved: bool) {
        if request < self.request || !self.pending.remove(key) {
            return;
        }
        if !saved {
            self.failed = true;
        }
    }

    fn stand(&mut self, key: &str, standing: Standing) {
        if !self.pending.contains(key) {
            return;
        }
        match standing {
            Standing::Editing => {}
            Standing::Parked => self.failed = true,
            Standing::Done => {
                self.pending.remove(key);
            }
        }
    }

    /// Whether every editor finished, once that is known: false as soon as one could not, and once
    /// the deadline passed.
    fn outcome(&self, now: Instant) -> Option<bool> {
        if self.failed {
            Some(false)
        } else if self.pending.is_empty() {
            Some(true)
        } else if now >= self.deadline {
            Some(false)
        } else {
            None
        }
    }
}

/// The panel's key among the editors of a `Finishing`.
const PANEL: &str = "main";
/// Asks `main` to leave the panel's editor, with its request number and note.
const RELEASE_PANEL: &str = "floating-release-panel-edit";

#[derive(Clone, Serialize)]
struct PanelRelease {
    request: u64,
    path: String,
}

/// The requests waiting. Taken under the registry, never the other way round.
static FINISHES: Mutex<Vec<Finishing>> = Mutex::new(Vec::new());
static FINISHED: Condvar = Condvar::new();
static NEXT_FINISH: AtomicU64 = AtomicU64::new(0);

fn finishes() -> MutexGuard<'static, Vec<Finishing>> {
    FINISHES.lock().unwrap_or_else(|e| e.into_inner())
}

/// How `key` stands in `registry`.
fn standing(registry: &Registry, key: &str) -> Standing {
    if key == PANEL {
        return match registry.panel_editing {
            Some(_) => Standing::Editing,
            None => Standing::Done,
        };
    }
    match registry.live.iter().find(|live| live.label == key) {
        Some(live) if live.editing && live.parked => Standing::Parked,
        Some(live) if live.editing => Standing::Editing,
        _ => Standing::Done,
    }
}

/// Asks every post-it editing and `picked` to finish, and waits; with `hold`, no edit begins
/// meanwhile, nor after, until `release_edits` or a folder choice ends. A parked one fails at once.
fn finish_where(hold: bool, picked: impl Fn(&Live) -> bool) -> bool {
    let request = NEXT_FINISH.fetch_add(1, Ordering::SeqCst) + 1;
    {
        let mut registry = registry();
        if hold {
            registry.held = true;
        }
        let mut editors = Vec::new();
        for live in registry
            .live
            .iter_mut()
            .filter(|live| live.editing && picked(live))
        {
            if live.parked {
                editors.push((live.label.clone(), Standing::Parked));
            } else {
                live.finish = request;
                live.changed();
                editors.push((live.label.clone(), Standing::Editing));
            }
        }
        // Still under the registry, so no answer arrives before its request waits.
        finishes().push(Finishing::new(
            request,
            editors,
            Instant::now() + FLUSH_WAIT,
        ));
    }
    wait_finished(request)
}

/// Asks the panel to leave its editor, and waits.
fn release_panel(app: &AppHandle) -> bool {
    let request = NEXT_FINISH.fetch_add(1, Ordering::SeqCst) + 1;
    let path = {
        let registry = registry();
        let Some(path) = registry.panel_editing.clone() else {
            return true;
        };
        finishes().push(Finishing::new(
            request,
            vec![(PANEL.to_owned(), Standing::Editing)],
            Instant::now() + FLUSH_WAIT,
        ));
        path
    };
    if let Err(error) = app.emit_to("main", RELEASE_PANEL, PanelRelease { request, path }) {
        eprintln!("[floating] panel not asked to finish: {error}");
        answer_finish(request, PANEL, false);
    }
    wait_finished(request)
}

/// Waits for the request `request` to end, and forgets it.
fn wait_finished(request: u64) -> bool {
    loop {
        let pending: Vec<String> = match finishes().iter().find(|f| f.request == request) {
            Some(finishing) => finishing.pending.iter().cloned().collect(),
            None => return false,
        };
        // Editors that stopped editing, went or were parked without answering, read apart from
        // `FINISHES` so neither lock waits for the other.
        let standings: Vec<(String, Standing)> = {
            let registry = registry();
            pending
                .into_iter()
                .map(|key| {
                    let standing = standing(&registry, &key);
                    (key, standing)
                })
                .collect()
        };
        let mut finishes = finishes();
        let Some(at) = finishes.iter().position(|f| f.request == request) else {
            return false;
        };
        for (key, standing) in standings {
            finishes[at].stand(&key, standing);
        }
        if let Some(finished) = finishes[at].outcome(Instant::now()) {
            finishes.remove(at);
            return finished;
        }
        // Woken by an answer, or after a while to read the registry again.
        let _ = FINISHED.wait_timeout(finishes, POLL);
    }
}

fn answer_finish(request: u64, key: &str, saved: bool) {
    for finishing in finishes().iter_mut() {
        finishing.answer(request, key, saved);
    }
    FINISHED.notify_all();
}

/// The post-it `label` answers the finish request `request`: it `saved` its note, which also ends
/// its edit as `end_edit` does, or could not and goes on editing.
pub fn finished(app: &AppHandle, label: &str, request: u64, saved: bool) -> Result<(), Error> {
    if with_live(label, |live| live.kind)? != Kind::Note {
        return Err(Error::unexpected("command restricted to post-its"));
    }
    if saved {
        end_edit(app, label)?;
    }
    answer_finish(request, label, saved);
    Ok(())
}

/// The panel answers the request `request` to leave its editor: it `released` it, saved, or could
/// not and goes on editing.
pub fn panel_released(request: u64, released: bool) {
    if released {
        registry().panel_edit(None);
    }
    answer_finish(request, PANEL, released);
}

/// Before exiting or choosing another notes folder: no edit begins until `release_edits`, or the
/// folder choice ends, and every post-it editing saves and returns to reading. Answers whether all
/// did; false at once for one parked, which cannot save until its folder returns. Waits, so never
/// on the main thread.
pub fn finish_edits() -> bool {
    finish_where(true, |_| true)
}

/// Edits may begin again: the exit or folder choice `finish_edits` held them for did not happen.
pub fn release_edits() {
    registry().held = false;
}

/// Something in the notes folder changed, `renames` among it as the watcher saw them in `root`
/// (`None` for the app's own notices): tells the worker, since this may run on the main thread.
pub fn notes_changed_requested(root: Option<std::path::PathBuf>, renames: Vec<(String, String)>) {
    send(Message::NotesChanged(root, renames));
}

/// The path a note at `path` ends at after `renames`, in the order they happened: a chain such as
/// `a` to `b` then `b` to `c` ends at `c`.
fn renamed(path: &str, renames: &[(String, String)]) -> Option<String> {
    let mut current: Option<String> = None;
    for (from, to) in renames {
        if same(current.as_deref().unwrap_or(path), from) {
            current = Some(to.clone());
        }
    }
    current
}

/// Post-its of a renamed note in `root` follow it, each saved by its id in the folder it opened
/// on, and every post-it reads its note again.
fn notes_changed(app: &AppHandle, root: Option<&std::path::Path>, renames: &[(String, String)]) {
    let mut moved: Vec<(FolderIdentity, Vec<(String, String)>)> = Vec::new();
    {
        let mut registry = registry();
        if root.is_some()
            && let Some(to) = registry
                .panel_editing
                .as_deref()
                .and_then(|path| renamed(path, renames))
        {
            registry.panel_editing = Some(to);
        }
        for live in registry
            .live
            .iter_mut()
            .filter(|live| live.kind == Kind::Note)
        {
            // Renames another folder's watcher saw, as one still flushing after a folder change,
            // name other notes.
            let here = root.is_some_and(|root| live.folder.is_at(root));
            if let Some(to) = here.then(|| renamed(&live.path, renames)).flatten() {
                live.path = to.clone();
                match moved.iter_mut().find(|(folder, _)| *folder == live.folder) {
                    Some((_, ids)) => ids.push((live.id.clone(), to)),
                    None => moved.push((live.folder.clone(), vec![(live.id.clone(), to)])),
                }
            }
            live.revision += 1;
            live.changed();
        }
        registry.refresh_blocked();
    }
    for (folder, paths) in moved {
        let saved = crate::notes::update_floating(app, &folder, |store| {
            for (id, path) in &paths {
                store.set_note_path(id, path);
            }
            Ok(())
        });
        if let Err(error) = saved {
            eprintln!("[floating] renamed note not followed: {error:?}");
        }
    }
    report(app);
}

pub fn set_opacity(label: &str, opacity: f64) -> Result<(), Error> {
    with_live(label, |live| {
        live.opacity = floats::clamp_opacity(opacity);
        live.changed();
    })?;
    send(Message::Changed(label.to_owned()));
    Ok(())
}

/// Lets every click through the window, or takes them again. Not saved: windows always reopen
/// taking clicks.
pub fn set_click_through(app: &AppHandle, label: &str, through: bool) -> Result<(), Error> {
    apply_click_through(label, through)?;
    send(Message::Wake);
    report(app);
    Ok(())
}

fn apply_click_through(label: &str, through: bool) -> Result<(), Error> {
    with_live(label, |live| {
        if live.raw != 0 {
            platform::set_click_through(live.raw, through);
        }
        live.click_through = through;
        live.revealed = false;
        live.changed();
    })
}

/// The image's size after a zoom, in CSS pixels, and whether it reached its largest.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Zoomed {
    width: f64,
    height: f64,
    at_max: bool,
}

/// Refuses a command of floating images to a post-it.
fn image_only(label: &str) -> Result<(), Error> {
    match with_live(label, |live| live.kind)? {
        Kind::Image => Ok(()),
        Kind::Note => Err(Error::unexpected("command restricted to floating images")),
    }
}

/// The shown window `label` and its image's proportions. Copied out, so Win32 calls never run
/// under the registry's lock.
fn shown(label: &str) -> Result<(isize, Aspect), Error> {
    let (raw, aspect, shown) =
        with_live(label, |live| (live.raw, live.aspect.clone(), live.shown))?;
    if raw == 0 || !shown {
        return Err(Error::unexpected("floating window not shown"));
    }
    Ok((raw, aspect))
}

/// The window's rectangle, its monitor, and the gutter and the shorter side's minimum there.
fn measured(raw: isize) -> Result<(Rect, platform::Monitor, i32, i32), Error> {
    let window = platform::window_rect(raw).ok_or_else(|| Error::unexpected("window gone"))?;
    let monitor = floats::monitor_of(window, &platform::monitors())
        .ok_or_else(|| Error::unexpected("no monitor"))?;
    let pad = scale_dpi(monitor.dpi, floats::SHADOW_PAD);
    let min = scale_dpi(monitor.dpi, floats::MIN_SIDE);
    Ok((window, monitor, pad, min))
}

/// Zooms the window `label` so its image is `width` CSS pixels wide, keeping its proportions,
/// within `floats::zoom_rect`'s bounds on the monitor that shows it, around the same center. Only
/// a window already shown, and never showing or activating it.
pub fn zoom(label: &str, width: f64) -> Result<Zoomed, Error> {
    image_only(label)?;
    let (raw, aspect) = shown(label)?;
    let (window, monitor, pad, min) = measured(raw)?;
    let scale = f64::from(monitor.dpi) / f64::from(platform::BASE_DPI);
    let (rect, at_max) =
        floats::zoom_rect(window, width * scale, aspect.get(), pad, min, monitor.work);
    platform::move_window(raw, rect);
    send(Message::Changed(label.to_owned()));
    let image = floats::inset(rect, pad);
    Ok(Zoomed {
        width: f64::from(image.right - image.left) / scale,
        height: f64::from(image.bottom - image.top) / scale,
        at_max,
    })
}

/// The image the window `label` shows is `width` by `height` pixels: when the proportions it
/// keeps differ, as for an entry saved before them or edited by hand, it takes the image's, and the
/// window shrinks to fit the image without moving its corner, showing nothing new.
pub fn natural(label: &str, width: u32, height: u32) -> Result<(), Error> {
    image_only(label)?;
    if width == 0 || height == 0 {
        return Ok(());
    }
    let (raw, aspect) = with_live(label, |live| (live.raw, live.aspect.clone()))?;
    let real = f64::from(width) / f64::from(height);
    if (real / aspect.get() - 1.0).abs() < 0.01 {
        return Ok(());
    }
    aspect.set(real);
    if raw != 0 {
        let (window, _, pad, _) = measured(raw)?;
        platform::move_window(raw, floats::contain(window, real, pad));
        send(Message::Changed(label.to_owned()));
    }
    Ok(())
}

/// Moves the window with the pointer, as its title bar would.
pub fn drag(window: &WebviewWindow) -> Result<(), Error> {
    with_live(window.label(), |_| ())?;
    Ok(window.start_dragging()?)
}

/// Resizes the window with the pointer from `direction`, an edge or corner such as `NorthEast`,
/// as its frame would.
pub fn resize(window: &WebviewWindow, direction: String) -> Result<(), Error> {
    with_live(window.label(), |_| ())?;
    // Tauri reads the direction from its name, as its own window command does.
    let direction =
        serde_json::from_value(serde_json::Value::String(direction)).map_err(Error::unexpected)?;
    Ok(window.as_ref().window().start_resize_dragging(direction)?)
}

/// From the tray: every window takes clicks again. Safe on the main thread.
pub fn unlock_all() {
    send(Message::UnlockAll);
}

/// A window event of a floating window, on the main thread: everything that takes the registry
/// happens on the worker.
pub fn window_event(label: &str, event: &tauri::WindowEvent) {
    if floats::parse_label(label).is_none() {
        return;
    }
    match event {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            send(Message::Close(label.to_owned()));
        }
        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
            send(Message::Changed(label.to_owned()));
        }
        tauri::WindowEvent::Destroyed => {
            forget_label(label);
            send(Message::Destroyed(label.to_owned()));
        }
        _ => {}
    }
}

/// Saves the moves and fades still waiting, before the app may exit, and waits until they are
/// written. Never on the main thread: the worker may be waiting for it, as while changing a
/// window's style.
pub fn flush() {
    let Some(worker) = WORKER.get() else {
        return;
    };
    let (done, finished) = mpsc::channel();
    if worker.send(Message::Flush(done)).is_err() {
        return;
    }
    if let Err(error) = finished.recv_timeout(FLUSH_WAIT) {
        eprintln!("[floating] last positions not saved before exit: {error}");
    }
}

fn windows() -> Vec<isize> {
    registry()
        .live
        .iter()
        .map(|live| live.raw)
        .filter(|&raw| raw != 0)
        .collect()
}

/// Hides every floating window without closing it, for M18's Windows Hello lock.
#[expect(
    dead_code,
    reason = "M18 hides floating windows until Windows Hello unlocks"
)]
pub fn hide_all() {
    for raw in windows() {
        platform::hide_window(raw);
    }
}

/// Shows again every floating window `hide_all` hid, where it was.
#[expect(
    dead_code,
    reason = "M18 shows floating windows once Windows Hello unlocks"
)]
pub fn show_all() {
    for raw in windows() {
        if let Some(rect) = platform::window_rect(raw) {
            platform::show_topmost(raw, rect);
        }
    }
}

/// Registers a live window opened on `folder`, as opening does, without building it.
#[cfg(test)]
pub(crate) fn register_for_tests(folder: &FolderIdentity) -> String {
    let epoch = registry().epoch;
    let entry = Entry {
        id: new_id(),
        kind: Kind::Image,
        path: "a.png".into(),
        x: 0,
        y: 0,
        width: 10,
        height: 10,
        opacity: 1.0,
        text_scale: None,
        collapsed: false,
    };
    reserve(epoch, &entry, folder).unwrap().0
}

#[cfg(test)]
pub(crate) fn take_for_tests(label: &str) {
    take(label);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn folder() -> FolderIdentity {
        FolderIdentity::default()
    }

    /// A registry holding one window opening on its current epoch, built as `raw` 7.
    fn opening() -> (Mutex<Registry>, String, u64) {
        let mut registry = Registry::new();
        let epoch = registry.epoch;
        let (label, _) = registry.reserve(epoch, &image("a.png"), &folder()).unwrap();
        registry.live[0].raw = 7;
        (Mutex::new(registry), label, epoch)
    }

    // Catches the image preview, a closed window or another window's label reaching a floating
    // window's commands, images or the barrier.
    #[test]
    fn only_a_live_floating_window_reaches_its_commands() {
        for label in ["image-preview", "main", "web", "float-424242"] {
            assert!(with_live(label, |_| ()).is_err(), "{label}");
            assert!(zoom(label, 400.0).is_err(), "{label}");
            assert!(natural(label, 400, 300).is_err(), "{label}");
            assert!(set_opacity(label, 0.5).is_err(), "{label}");
            assert!(set_collapsed(label, true).is_err(), "{label}");
            assert!(registry().begin_edit(label).is_err(), "{label}");
            assert!(registry().end_edit(label).is_err(), "{label}");
            assert!(copies(label).is_err(), "{label}");
            assert!(!is_live(label), "{label}");
        }
        let epoch = registry().epoch;
        let (label, _) = reserve(epoch, &image("a.png"), &folder()).unwrap();
        assert!(is_live(&label));
        assert!(crate::is_secondary_window(&label));
        assert!(crate::main_window::reads_images(&label));
        assert!(with_live(&label, |_| ()).is_ok());
        take(&label);
        assert!(!is_live(&label));
        assert!(!crate::is_secondary_window(&label));
        assert!(!crate::main_window::reads_images(&label));
        assert!(with_live(&label, |_| ()).is_err());
        assert!(crate::is_secondary_window("image-preview"));
    }

    // Catches an opening or a restore started before a vault locked, or before another folder was
    // chosen or trusted, still registering or showing its window afterwards.
    #[test]
    fn work_started_before_a_folder_change_never_opens() {
        let (registry, label, epoch) = opening();
        assert!(lock(&registry).revoke().len() == 1);
        assert!(!show_if_current(&registry, &label, epoch, |_, _| panic!(
            "shown"
        )));
        let reserved = lock(&registry).reserve(epoch, &image("b.png"), &folder());
        assert_eq!(reserved.err(), Some(Error::FolderChanged));
        let epoch = lock(&registry).epoch;
        assert!(
            lock(&registry)
                .reserve(epoch, &image("b.png"), &folder())
                .is_ok()
        );
    }

    #[test]
    fn at_most_eight_windows_open() {
        let mut registry = Registry::new();
        for _ in 0..MAX_FLOATING {
            assert!(registry.reserve(0, &image("a.png"), &folder()).is_ok());
        }
        let refused = registry.reserve(0, &image("a.png"), &folder());
        assert_eq!(refused.err(), Some(Error::FloatingLimit { max: 8 }));
    }

    /// Saves and shows the window of `opening` while the user closes it (`close`) or a folder
    /// change begins, before or after its entry is saved. Answers how many entries stay saved and whether
    /// the window showed.
    fn open_interrupted(close: bool, after_save: bool) -> (usize, bool) {
        let (registry, label, epoch) = opening();
        let id = lock(&registry).live[0].id.clone();
        let store = RefCell::new(floats::Store::default());
        let interrupt = || {
            if close {
                // As `close`: the window is marked closing, its entry goes under `SAVING`, then
                // it leaves the registry.
                lock(&registry).mark_closing_where(|live| live.label == label);
                store.borrow_mut().remove(&[&id]);
                lock(&registry).take(&label);
            } else {
                lock(&registry).begin_switch();
            }
        };
        let mut shown = false;
        let finished = save_and_show(
            &registry,
            &label,
            epoch,
            |admit| {
                if !after_save {
                    interrupt();
                }
                admit()?;
                store.borrow_mut().add(Entry {
                    id: id.clone(),
                    kind: Kind::Image,
                    path: "a.png".into(),
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 10,
                    opacity: 1.0,
                    text_scale: None,
                    collapsed: false,
                });
                // Still under `SAVING`: a close's own removal waits for it, so it lands after this.
                if after_save {
                    interrupt();
                }
                Ok(())
            },
            |_, _| shown = true,
        );
        assert_eq!(finished.is_ok(), shown);
        (store.borrow().entries().count(), shown)
    }

    // Catches an opening saving or showing a window the user is already closing.
    #[test]
    fn a_closing_window_is_neither_saved_nor_shown() {
        let (registry, label, epoch) = opening();
        lock(&registry).mark_closing_where(|live| live.label == label);
        assert_eq!(
            lock(&registry).admits(&label, epoch),
            Err(Error::FolderChanged)
        );
        assert_eq!(lock(&registry).confirm(&label, epoch), None);
    }

    // Catches a restored window shown with the proportions it was saved with after its image had
    // told its own.
    #[test]
    fn a_window_shows_with_the_proportions_its_image_told() {
        let (registry, label, epoch) = opening();
        lock(&registry).live[0].aspect.set(2.0);
        let mut told = 0.0;
        assert!(show_if_current(&registry, &label, epoch, |_, aspect| {
            told = aspect
        }));
        assert_eq!(told, 2.0);
    }

    // Catches a window closed while it was opening coming back at the next start.
    #[test]
    fn a_window_closed_while_opening_stays_closed() {
        assert_eq!(open_interrupted(true, false), (0, false));
        assert_eq!(open_interrupted(true, true), (0, false));
    }

    // Catches an opening saving into, or showing over, a folder chosen while it was on its way.
    #[test]
    fn a_folder_change_before_saving_leaves_nothing() {
        assert_eq!(open_interrupted(false, false), (0, false));
    }

    // Catches a window saved just before its folder changed showing anyway, or its entry being
    // dropped from the folder it rightly opened on.
    #[test]
    fn a_folder_change_after_saving_keeps_the_entry_and_hides_the_window() {
        assert_eq!(open_interrupted(false, true), (1, false));
    }

    #[test]
    fn an_opening_left_alone_saves_then_shows() {
        let (registry, label, epoch) = opening();
        let saved = RefCell::new(false);
        let mut shown_saved = false;
        let finished = save_and_show(
            &registry,
            &label,
            epoch,
            |admit| {
                admit()?;
                *saved.borrow_mut() = true;
                Ok(())
            },
            |raw, _| shown_saved = raw == 7 && *saved.borrow(),
        );
        assert!(finished.is_ok());
        assert!(shown_saved);
    }

    // Catches an opening or restore registering a window while another folder is being chosen
    // or trusted, between the old windows closing and the settings changing.
    #[test]
    fn nothing_opens_while_a_folder_is_being_chosen() {
        let mut registry = Registry::new();
        let epoch = registry.epoch;
        registry.begin_switch();
        let during = registry.epoch;
        assert!(!registry.current(during));
        let reserved = registry.reserve(during, &image("a.png"), &folder());
        assert_eq!(reserved.err(), Some(Error::FolderChanged));
        assert!(registry.end_switch());
        assert!(!registry.current(epoch));
        assert!(!registry.current(during));
        let after = registry.epoch;
        assert!(registry.reserve(after, &image("a.png"), &folder()).is_ok());
    }

    // Regression: a folder choice that revoked a window the user was closing kept its entry, so
    // the closed window came back with that folder.
    #[test]
    fn a_close_overtaken_by_a_folder_change_is_still_forgotten() {
        let mut registry = Registry::new();
        let mut store = floats::Store::default();
        for path in ["a.png", "b.png"] {
            let id = new_id();
            registry
                .reserve(
                    0,
                    &Entry {
                        id: id.clone(),
                        ..image(path)
                    },
                    &folder(),
                )
                .unwrap();
            store.add(Entry {
                id,
                kind: Kind::Image,
                path: path.into(),
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                opacity: 1.0,
                text_scale: None,
                collapsed: false,
            });
        }
        let label = registry.live[0].label.clone();
        let closed = registry
            .mark_closing_where(|live| live.label == label)
            .remove(0)
            .id;
        // The folder change revokes both before the close saved anything.
        let gone = registry.begin_switch();
        let forgotten = closing(&gone);
        assert_eq!(forgotten.len(), 1);
        assert_eq!(forgotten[0].1, [closed.as_str()]);
        store.remove(&forgotten[0].1);
        let kept: Vec<_> = store.entries().map(|entry| entry.path.as_str()).collect();
        assert_eq!(kept, ["b.png"]);
    }

    fn image(path: &str) -> Entry {
        Entry {
            id: new_id(),
            kind: Kind::Image,
            path: path.into(),
            x: 0,
            y: 0,
            width: 10,
            height: 10,
            opacity: 1.0,
            text_scale: None,
            collapsed: false,
        }
    }

    // Catches the badge of an image left open, or another image's window closing, when the user
    // closes an image's floating window from the note, however the path is spelled.
    #[test]
    fn closing_a_path_closes_only_the_windows_of_that_image() {
        let mut registry = Registry::new();
        for path in [
            "attachments/Map.png",
            "attachments//map.png",
            "attachments/b.png",
        ] {
            registry.reserve(0, &image(path), &folder()).unwrap();
        }
        let closing = registry.mark_closing_where(same_path("./attachments/MAP.png"));
        let labels: Vec<_> = closing.iter().map(|closed| closed.label.as_str()).collect();
        assert_eq!(labels, ["float-1", "float-2"]);
        assert_eq!(registry.paths(), ["attachments/b.png"]);
        assert!(
            registry
                .mark_closing_where(same_path("../map.png"))
                .is_empty()
        );
    }

    // Catches closing every window dropping entries a later build saved, such as post-its.
    #[test]
    fn closing_every_window_keeps_entries_of_other_kinds() {
        let mut registry = Registry::new();
        let note = serde_json::json!({ "id": new_id(), "kind": "note", "path": "a.md" });
        let first = image("attachments/a.png");
        let second = image("attachments/b.png");
        let file = serde_json::json!({ "version": 1, "entries": [note, first, second] });
        let mut store = floats::parse(&serde_json::to_vec(&file).unwrap()).unwrap();
        for entry in [&first, &second] {
            registry.reserve(0, entry, &folder()).unwrap();
        }
        let closing = registry.mark_closing_where(|_| true);
        assert_eq!(closing.len(), 2);
        assert!(registry.paths().is_empty());
        let ids: Vec<_> = closing.iter().map(|closed| closed.id.as_str()).collect();
        store.remove(&ids);
        assert_eq!(store.entries().count(), 0);
        let saved: serde_json::Value = serde_json::from_slice(&store.to_bytes()).unwrap();
        assert_eq!(saved["entries"], serde_json::json!([note]));
    }

    // Catches a zoom revealing a window still hidden while it opens.
    #[test]
    fn a_window_not_shown_yet_never_zooms() {
        let label = register_for_tests(&folder());
        // Built, so only `shown` holds the zoom back.
        with_live(&label, |live| live.raw = 7).unwrap();
        assert!(zoom(&label, 400.0).is_err());
        take(&label);
    }

    // Catches a floating image sizing text it does not have, or a post-it being zoomed or reshaped
    // to an image's proportions.
    #[test]
    fn each_kind_reaches_only_its_own_commands() {
        let picture = register_for_tests(&folder());
        let epoch = registry().epoch;
        let note = Entry {
            kind: Kind::Note,
            path: "a.md".into(),
            ..image("a.md")
        };
        let (post_it, _) = reserve(epoch, &note, &folder()).unwrap();
        assert!(set_text_scale(&picture, 1.5).is_err());
        assert!(set_text_scale(&post_it, 1.5).is_ok());
        assert!(zoom(&post_it, 400.0).is_err());
        assert!(natural(&post_it, 400, 300).is_err());
        with_live(&picture, |live| (live.raw, live.shown) = (7, true)).unwrap();
        assert_eq!(
            set_collapsed(&picture, true),
            Err(Error::unexpected("command restricted to post-its"))
        );
        let refused = Err(Error::unexpected("command restricted to post-its"));
        assert_eq!(registry().begin_edit(&picture), refused);
        assert_eq!(registry().end_edit(&picture), refused.map(|_| None));
        take(&picture);
        take(&post_it);
    }

    // Regression: renames saved pair by pair, in window order, sent two post-its to one note.
    #[test]
    fn a_post_it_follows_its_renames_in_the_order_they_happened() {
        let renames = [
            ("b.md".to_owned(), "c.md".to_owned()),
            ("A.md".to_owned(), "b.md".to_owned()),
        ];
        assert_eq!(renamed("a.md", &renames), Some("b.md".into()));
        assert_eq!(renamed("b.md", &renames), Some("c.md".into()));
        assert_eq!(renamed("x.md", &renames), None);
        let chain = [
            ("a.md".to_owned(), "b.md".to_owned()),
            ("b.md".to_owned(), "c.md".to_owned()),
        ];
        assert_eq!(renamed("a.md", &chain), Some("c.md".into()));
    }

    // Catches one floating window's command changing another's opacity or click-through.
    #[test]
    fn a_window_changes_only_itself() {
        let first = register_for_tests(&folder());
        let second = register_for_tests(&folder());
        set_opacity(&first, 0.4).unwrap();
        apply_click_through(&first, true).unwrap();
        let state = |label: &str| with_live(label, |live| (live.opacity, live.click_through));
        assert_eq!(state(&first), Ok((0.4, true)));
        assert_eq!(state(&second), Ok((1.0, false)));
        take(&first);
        assert!(set_opacity(&first, 0.2).is_err());
        assert_eq!(state(&second), Ok((1.0, false)));
        take(&second);
    }
    fn note(path: &str) -> Entry {
        Entry {
            kind: Kind::Note,
            ..image(path)
        }
    }

    /// A registry holding a shown post-it for each of `paths`, labeled from `float-1`.
    fn post_its(paths: &[&str]) -> Registry {
        let mut registry = Registry::new();
        for path in paths {
            registry.reserve(0, &note(path), &folder()).unwrap();
        }
        for live in &mut registry.live {
            (live.raw, live.shown) = (7, true);
        }
        registry
    }

    fn begins(registry: &mut Registry, label: &str) -> bool {
        registry.begin_edit(label).unwrap().is_some()
    }

    fn blocked(registry: &Registry, label: &str) -> bool {
        registry
            .live
            .iter()
            .any(|live| live.label == label && live.blocked)
    }

    // Catches a post-it editing the note the panel edits, written however the panel spelled it,
    // so two editors overwrite each other.
    #[test]
    fn a_note_the_panel_edits_is_never_edited_in_a_post_it() {
        let mut registry = post_its(&["Inbox/a.md"]);
        assert!(registry.panel_edit(Some("inbox//A.md".into())));
        assert!(!begins(&mut registry, "float-1"));
        assert!(blocked(&registry, "float-1"));
        assert!(registry.panel_edit(None));
        assert!(!blocked(&registry, "float-1"));
        assert!(begins(&mut registry, "float-1"));
    }

    // Catches the panel editing a note a post-it edits, and the transfer losing who to ask.
    #[test]
    fn a_note_a_post_it_edits_is_never_edited_in_the_panel() {
        let mut registry = post_its(&["Inbox/a.md"]);
        assert!(begins(&mut registry, "float-1"));
        assert!(!registry.panel_edit(Some("./Inbox/a.md".into())));
        assert_eq!(
            registry.may_edit(&Editor::Panel, "Inbox/a.md"),
            Err(Refusal::Editing(Editor::Float("float-1".into())))
        );
        assert_eq!(registry.editing_paths(), ["Inbox/a.md"]);
    }

    // Catches two post-its of one note both editing it.
    #[test]
    fn a_note_is_edited_in_one_post_it_at_a_time() {
        let mut registry = post_its(&["a.md", "A.md"]);
        assert!(begins(&mut registry, "float-1"));
        assert!(!begins(&mut registry, "float-2"));
        assert!(blocked(&registry, "float-2"));
        assert!(!blocked(&registry, "float-1"));
    }

    // Catches an edit that ended, or whose post-it went, still holding its note.
    #[test]
    fn ending_an_edit_releases_its_note() {
        let mut registry = post_its(&["a.md", "a.md"]);
        assert!(begins(&mut registry, "float-1"));
        let ended = Ended {
            raw: 7,
            previous_focus: None,
            parked: false,
        };
        assert_eq!(registry.end_edit("float-1"), Ok(Some(ended)));
        assert_eq!(registry.end_edit("float-1"), Ok(None));
        assert!(begins(&mut registry, "float-2"));
        registry.take("float-2");
        assert!(!blocked(&registry, "float-1"));
        assert!(registry.panel_edit(Some("a.md".into())));
    }

    // Catches an edit beginning while the app exits or another notes folder is chosen.
    #[test]
    fn no_edit_begins_while_edits_are_held() {
        let mut registry = post_its(&["a.md"]);
        registry.held = true;
        assert!(!begins(&mut registry, "float-1"));
        assert!(!registry.panel_edit(Some("b.md".into())));
        registry.held = false;
        registry.switching = 1;
        assert!(!begins(&mut registry, "float-1"));
        assert!(!registry.panel_edit(Some("b.md".into())));
    }

    // Catches the lease of one note holding off the others.
    #[test]
    fn different_notes_are_edited_independently() {
        let mut registry = post_its(&["a.md", "b.md"]);
        assert!(registry.panel_edit(Some("c.md".into())));
        assert!(begins(&mut registry, "float-1"));
        assert!(begins(&mut registry, "float-2"));
        assert!(!blocked(&registry, "float-1"));
        assert_eq!(registry.editing_paths(), ["a.md", "b.md"]);
    }

    // Catches a post-it saving a note it does not edit, a floating image saving one, or a window
    // Rust never registered reaching the save.
    #[test]
    fn only_a_post_it_editing_its_note_saves_it() {
        let mut registry = post_its(&["a.md"]);
        registry.reserve(0, &image("a.png"), &folder()).unwrap();
        for label in ["float-1", "float-2", "float-424242", "main"] {
            assert!(registry.editing(label, 1).is_err(), "{label}");
        }
        assert!(begins(&mut registry, "float-1"));
        assert_eq!(
            registry.editing("float-1", 1),
            Ok(("a.md".into(), folder()))
        );
        registry.take("float-1");
        assert!(registry.editing("float-1", 1).is_err());
    }

    // Catches a reading or parked post-it, or a floating image, writing the clipboard: only an
    // editor copies, so a page in a window that edits nothing never reaches it.
    #[test]
    fn only_a_post_it_editing_its_note_copies() {
        let mut registry = post_its(&["a.md"]);
        registry.reserve(0, &image("a.png"), &folder()).unwrap();
        assert!(registry.copies("float-1").is_err());
        assert!(registry.copies("float-2").is_err());
        assert!(begins(&mut registry, "float-1"));
        assert_eq!(registry.copies("float-1"), Ok(()));
        registry.revoke();
        assert!(registry.copies("float-1").is_err());
    }

    // Regression: a save still waiting for `SAVING` when its edit ended wrote, once a new edit of
    // the same post-it began, the draft the user had left behind.
    #[test]
    fn a_save_of_an_edit_that_ended_is_refused_in_the_next() {
        let mut registry = post_its(&["a.md"]);
        let first = registry.begin_edit("float-1").unwrap().unwrap().lease;
        registry.end_edit("float-1").unwrap();
        let second = registry.begin_edit("float-1").unwrap().unwrap().lease;
        assert!(registry.editing("float-1", first).is_err());
        assert!(registry.editing("float-1", second).is_ok());
    }

    // Catches a vault that locks destroying a post-it with unsaved text, or leaving a post-it that
    // was only read showing its note.
    #[test]
    fn a_folder_change_parks_the_post_its_editing_and_takes_the_rest() {
        let mut registry = post_its(&["a.md", "b.md"]);
        registry.reserve(0, &image("c.png"), &folder()).unwrap();
        assert!(begins(&mut registry, "float-1"));
        let gone: Vec<_> = registry
            .revoke()
            .into_iter()
            .map(|live| live.label)
            .collect();
        assert_eq!(gone, ["float-2", "float-3"]);
        assert_eq!(registry.live.len(), 1);
        assert!(registry.live[0].parked && registry.live[0].editing);
        assert!(registry.begin_switch().is_empty());
        assert!(registry.live[0].parked);
        assert!(registry.paths().is_empty());
        // Ending its edit, once saved or discarded, lets it go as a revoked window.
        let ended = registry.end_edit("float-1").unwrap().unwrap();
        assert!(ended.parked);
    }

    // Catches a parked post-it's moves being saved into the folder that came back, or another.
    #[test]
    fn a_parked_post_it_saves_no_moves() {
        let mut registry = post_its(&["a.md"]);
        assert!(begins(&mut registry, "float-1"));
        registry.revoke();
        let epoch = registry.live[0].epoch;
        // The worker saves a move only on the registry's epoch.
        assert_ne!(epoch, registry.epoch);
        assert!(!registry.current(epoch));
    }

    // Catches a parked post-it reaching, or showing, a folder that came back as another one.
    #[test]
    fn only_the_same_folder_adopts_a_parked_post_it() {
        let mut registry = Registry::new();
        let vault = FolderIdentity::for_tests("V:/notes", "1");
        let other = FolderIdentity::for_tests("V:/notes", "2");
        registry.reserve(0, &note("a.md"), &vault).unwrap();
        (registry.live[0].raw, registry.live[0].shown) = (7, true);
        assert!(begins(&mut registry, "float-1"));
        registry.revoke();
        registry.adopt(&other);
        assert!(registry.live[0].parked);
        assert!(!begins(&mut registry, "float-1"));
        registry.adopt(&vault);
        assert!(!registry.live[0].parked);
        assert!(registry.current(registry.live[0].epoch));
        assert_eq!(registry.live[0].revision, 1);
    }

    fn finishing(editors: &[(&str, Standing)]) -> (Finishing, Instant) {
        let now = Instant::now();
        let editors = editors
            .iter()
            .map(|(key, standing)| ((*key).to_owned(), *standing))
            .collect();
        (Finishing::new(5, editors, now + FLUSH_WAIT), now)
    }

    // Catches the app exiting, or another folder being chosen, over a post-it parked with text
    // it cannot save until its folder returns.
    #[test]
    fn a_parked_post_it_never_finishes() {
        let (finish, now) = finishing(&[
            ("float-1", Standing::Editing),
            ("float-2", Standing::Parked),
        ]);
        assert_eq!(finish.outcome(now), Some(false));
    }

    // Catches a post-it that never answers being taken as saved.
    #[test]
    fn a_post_it_that_never_answers_fails_its_finish() {
        let (mut finish, now) = finishing(&[
            ("float-1", Standing::Editing),
            ("float-2", Standing::Editing),
        ]);
        finish.answer(5, "float-1", true);
        assert_eq!(finish.outcome(now), None);
        assert_eq!(finish.outcome(now + FLUSH_WAIT), Some(false));
    }

    #[test]
    fn every_editor_saved_finishes() {
        let (mut finish, now) =
            finishing(&[("float-1", Standing::Editing), (PANEL, Standing::Editing)]);
        finish.answer(5, "float-1", true);
        // An edit ended without an answer, as discarded, leaves nothing to wait for.
        finish.stand(PANEL, Standing::Done);
        assert_eq!(finish.outcome(now), Some(true));
    }

    // Catches a save that failed, or a late answer to an earlier request, deciding this one.
    #[test]
    fn a_failed_save_fails_and_a_stale_answer_is_ignored() {
        let (mut finish, now) = finishing(&[("float-1", Standing::Editing)]);
        finish.answer(4, "float-1", true);
        assert_eq!(finish.outcome(now), None);
        finish.answer(5, "float-1", false);
        assert_eq!(finish.outcome(now), Some(false));
    }

    // Catches asking to edit a note edited elsewhere refusing instead of moving the edit, or
    // asking a post-it that cannot save to finish.
    #[test]
    fn asking_to_edit_a_note_edited_elsewhere_moves_the_edit() {
        let mut registry = post_its(&["a.md", "a.md", "b.md"]);
        assert_eq!(registry.begin_transfer("float-1"), Ok(Transfer::Grant));
        assert!(begins(&mut registry, "float-1"));
        let holder = Transfer::Finish(Editor::Float("float-1".into()));
        assert_eq!(registry.begin_transfer("float-2"), Ok(holder));
        let holder = Transfer::Finish(Editor::Float("float-1".into()));
        assert_eq!(registry.transfer(&Editor::Panel, "A.md"), holder);
        assert!(registry.panel_edit(Some("b.md".into())));
        assert_eq!(
            registry.begin_transfer("float-3"),
            Ok(Transfer::Finish(Editor::Panel))
        );
        registry.held = true;
        assert_eq!(registry.begin_transfer("float-2"), Ok(Transfer::Refuse));
        registry.held = false;
        registry.revoke();
        assert_eq!(registry.transfer(&Editor::Panel, "a.md"), Transfer::Refuse);
    }

    // Catches a close from outside a post-it (Alt+F4, the panel, the tray) destroying its unsaved
    // text.
    #[test]
    fn a_close_only_asks_an_editing_post_it_to_close() {
        let mut registry = post_its(&["a.md", "b.md"]);
        assert!(begins(&mut registry, "float-1"));
        let closing = registry.mark_closing_where(|_| true);
        let labels: Vec<_> = closing.iter().map(|closed| closed.label.as_str()).collect();
        assert_eq!(labels, ["float-2"]);
        assert!(!registry.live[0].closing);
        assert_eq!(registry.live[0].close_requested, 1);
    }

    // Catches a post-it taking focus to edit while clicks go through it, or before it shows.
    #[test]
    fn only_a_shown_post_it_taking_clicks_begins_editing() {
        let mut registry = post_its(&["a.md"]);
        registry.live[0].click_through = true;
        assert!(!begins(&mut registry, "float-1"));
        registry.live[0].click_through = false;
        registry.live[0].shown = false;
        assert!(!begins(&mut registry, "float-1"));
        assert!(registry.panel_edit(Some("a.md".into())));
    }
}
