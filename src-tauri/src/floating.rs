//! Floating windows: note images detached from the panel into borderless, topmost windows that
//! never activate, with their own opacity and a click-through mode, reopened where they were from
//! `.docdock/floating.json`. The rules are pure in `floats.rs`; this runs their effects. Each window
//! is a secondary app window ([0014](../../docs/decisions/0014-app-windows.md)) labeled `float-<n>`
//! for this run: it has no capability, reaches only the `float_*` commands, and only for itself,
//! and hears what to show only through the IPC channel it hands over with `float_state`.
//!
//! Every window belongs to the notes folder it opened on (`notes::FolderIdentity`, its path and
//! marker id) and to the registry's epoch, which changes whenever that folder leaves or another is
//! chosen: work started on an earlier epoch never registers, shows or saves a window, and anything
//! it would write must name the folder still configured.
//!
//! One worker thread saves moves and fades after a pause and, while any window lets clicks
//! through, watches Alt and the cursor so holding Alt over such a window reaches its controls.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::dock::Request;
use crate::error::Error;
use crate::floats::{self, Entry, Kind, MAX_FLOATING};
use crate::i18n::Locale;
use crate::notes::FolderIdentity;
use crate::platform::{self, Rect, scale_dpi};

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
    path: Option<String>,
    opacity: f64,
    click_through: bool,
    /// Alt is held over the click-through window: it takes clicks and shows its controls.
    revealed: bool,
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
    path: String,
    /// The window, once built and prepared; 0 before.
    raw: isize,
    /// The image's width over its height, which the window keeps when resized or zoomed; shared
    /// with its `WM_SIZING` hook.
    aspect: Aspect,
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
    click_through: bool,
    revealed: bool,
    /// Changes with what the window shows.
    version: u64,
    /// The window's channel, from its latest load.
    channel: Option<Channel<Update>>,
}

impl Live {
    fn update(&self) -> Update {
        Update {
            version: self.version,
            path: Some(self.path.clone()),
            opacity: self.opacity,
            click_through: self.click_through,
            revealed: self.revealed,
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
}

impl Registry {
    const fn new() -> Self {
        Self {
            next: 0,
            epoch: 0,
            switching: 0,
            live: Vec::new(),
        }
    }

    /// Work started on `epoch` may still open windows: no folder change came since, and none is
    /// under way.
    fn current(&self, epoch: u64) -> bool {
        epoch == self.epoch && self.switching == 0
    }

    /// Takes a label and a place for a window still to be built, for work started on `epoch`;
    /// answers its label and how many windows were open already.
    fn reserve(
        &mut self,
        epoch: u64,
        id: &str,
        path: &str,
        opacity: f64,
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
            id: id.to_owned(),
            path: path.to_owned(),
            raw: 0,
            aspect: Aspect::new(1.0),
            shown: false,
            folder: folder.clone(),
            epoch,
            closing: false,
            opacity: floats::clamp_opacity(opacity),
            click_through: false,
            revealed: false,
            version: 1,
            channel: None,
        });
        Ok((label, open))
    }

    fn take(&mut self, label: &str) -> Option<Live> {
        let at = self.live.iter().position(|live| live.label == label)?;
        Some(self.live.remove(at))
    }

    /// Starts a new epoch and takes every window.
    fn revoke(&mut self) -> Vec<Live> {
        self.epoch += 1;
        std::mem::take(&mut self.live)
    }

    /// Marks every window `picked` closed by the user; answers their labels, ids and folders.
    fn mark_closing_where(&mut self, picked: impl Fn(&Live) -> bool) -> Vec<Closing> {
        self.live
            .iter_mut()
            .filter(|live| picked(live))
            .map(|live| {
                live.closing = true;
                Closing {
                    label: live.label.clone(),
                    id: live.id.clone(),
                    folder: live.folder.clone(),
                }
            })
            .collect()
    }

    /// The images the open windows show, as saved.
    fn paths(&self) -> Vec<String> {
        let mut paths: Vec<_> = self
            .live
            .iter()
            .filter(|live| !live.closing)
            .map(|live| live.path.clone())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// A folder choice begins: as `revoke`, and nothing opens until it ends.
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
        let here = self.live.iter().any(|live| live.label == label);
        if self.current(epoch) && here {
            Ok(())
        } else {
            Err(Error::FolderChanged)
        }
    }

    /// The built window `label`, if it may show now: no close or folder change came since the
    /// opening started on `epoch`.
    fn confirm(&self, label: &str, epoch: u64) -> Option<isize> {
        if !self.current(epoch) {
            return None;
        }
        self.live
            .iter()
            .find(|live| live.label == label && live.raw != 0)
            .map(|live| live.raw)
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
}

fn send(message: Message) {
    if let Some(worker) = WORKER.get() {
        let _ = worker.send(message);
    }
}

/// A move or fade waiting to be saved.
struct Dirty {
    rect: Rect,
    opacity: f64,
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
                    let dirty = Dirty {
                        rect,
                        opacity: live.opacity,
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
                    store.update(id, dirty.rect, dirty.opacity);
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

/// The tray's "Unlock floating windows" is available while any window lets clicks through.
/// Tells the tray and `main` what changed: "Unlock floating windows" is available while any
/// window lets clicks through and "Close all floating windows" while any is open, and `main` marks
/// the images floating windows show (`floating-paths`).
fn report(app: &AppHandle) {
    let (through, open, paths) = {
        let registry = registry();
        let through = registry.live.iter().any(|live| live.click_through);
        (through, !registry.live.is_empty(), registry.paths())
    };
    crate::tray::set_floating_items(app, through, open);
    if let Err(error) = app.emit_to("main", PATHS, paths) {
        eprintln!("[floating] open images not reported: {error}");
    }
}

/// Tells `main` which images floating windows show.
const PATHS: &str = "floating-paths";

/// The images the open floating windows show, relative to the notes folder.
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
fn reserve(
    epoch: u64,
    id: &str,
    path: &str,
    opacity: f64,
    folder: &FolderIdentity,
) -> Result<(String, usize), Error> {
    let mut registry = registry();
    let reserved = registry.reserve(epoch, id, path, opacity, folder)?;
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
fn build(app: &AppHandle, label: &str, aspect: f64) -> Result<(), Error> {
    let side = f64::from(floats::MIN_SIDE);
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
        .min_inner_size(side, side);
    let window = crate::main_window::secondary_window(app, builder)?;
    let raw = match window.hwnd() {
        Ok(hwnd) => hwnd.0 as isize,
        Err(error) => {
            let _ = window.destroy();
            return Err(Error::unexpected(error));
        }
    };
    platform::prepare_floating(raw);
    // Resizing keeps the image's proportions, so the window never shows margins around it.
    let aspect = Aspect::new(aspect);
    let hooked = aspect.clone();
    let sized = window.run_on_main_thread(move || {
        let sizing: platform::Sizing = Box::new(move |rect, edge, dpi, work| {
            let pad = scale_dpi(dpi, floats::SHADOW_PAD);
            let min = scale_dpi(dpi, floats::MIN_SIDE);
            floats::constrain_sizing(rect, edge, hooked.get(), pad, min, work)
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
    show: impl FnOnce(isize),
) -> bool {
    let mut registry = lock(registry);
    let Some(raw) = registry.confirm(label, epoch) else {
        return false;
    };
    show(raw);
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
    show: impl FnOnce(isize),
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
    let epoch = registry().epoch;
    let folder = crate::notes::floating_folder(app, &path)?;
    let id = new_id();
    let (label, open) = reserve(epoch, &id, &path, 1.0, &folder)?;
    let opened = place(requests, thumb, natural).and_then(|placed| {
        let aspect = match natural {
            (width, height) if width > 0 && height > 0 => f64::from(width) / f64::from(height),
            _ => floats::aspect_of(placed.image),
        };
        let pad = scale_dpi(placed.dpi, floats::SHADOW_PAD);
        let window = floats::outset(placed.image, pad);
        let window = floats::cascade(window, open, placed.dpi, placed.side, placed.free);
        let rect = fitted(window, aspect, &platform::monitors());
        let mut entry = Entry {
            id: id.clone(),
            kind: Kind::Image,
            path,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            opacity: 1.0,
        };
        entry.set_rect(rect);
        build(app, &label, aspect)?;
        save_and_show(
            &REGISTRY,
            &label,
            epoch,
            |admit| crate::notes::add_floating(app, &folder, entry, admit),
            |raw| show(raw, rect),
        )
    });
    if opened.is_err() {
        take(&label);
        destroy(app, &label);
    }
    report(app);
    opened
}

/// `window`, with its image of `aspect`, on the monitors there are now and reshaped to the image.
fn fitted(window: Rect, aspect: f64, monitors: &[platform::Monitor]) -> Rect {
    let rect = floats::fit_to_monitors(window, monitors);
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

/// Opens a window for the saved `entry` at `rect`, for work started on `epoch`.
fn open_saved(
    app: &AppHandle,
    entry: &Entry,
    rect: Rect,
    epoch: u64,
    folder: &FolderIdentity,
) -> Result<(), Error> {
    let (label, _) = reserve(epoch, &entry.id, &entry.path, entry.opacity, folder)?;
    let dpi = floats::monitor_of(rect, &platform::monitors()).map_or(platform::BASE_DPI, |m| m.dpi);
    let aspect = floats::aspect_of(floats::inset(rect, scale_dpi(dpi, floats::SHADOW_PAD)));
    let shown = build(app, &label, aspect).and_then(|()| {
        show_if_current(&REGISTRY, &label, epoch, |raw| show(raw, rect))
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
/// monitors there are now allow. Entries whose image is gone are dropped; those whose image cannot
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
        Ok((folder, Some(store))) => (folder, store),
        Ok((_, None)) => return,
        Err(error) => {
            eprintln!("[floating] saved windows not read: {error:?}");
            return;
        }
    };
    let monitors = platform::monitors();
    let mut gone = Vec::new();
    for entry in store.entries() {
        let open = {
            let registry = registry();
            if !registry.current(epoch) {
                return;
            }
            registry.live.iter().any(|live| live.id == entry.id)
        };
        if open {
            continue;
        }
        match crate::notes::image_present(app, &folder, &entry.path) {
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
        let rect = fitted(saved, aspect, &monitors);
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
        eprintln!("[floating] gone images not dropped: {error:?}");
    }
}

/// The notes folder went away, such as a vault that locked, or another is about to be chosen: a
/// new epoch starts, every window stops reaching its commands and images at once, then goes; its
/// saved entry stays for when the folder returns
/// ([0009](../../docs/decisions/0009-note-privacy.md)). Openings still on their way give up.
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
        if registry().end_switch() {
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

/// The user closes every window showing `path`, however it is spelled, as `close` does.
pub fn close_path(app: &AppHandle, path: &str) {
    let closing = registry().mark_closing_where(same_image(path));
    forget_closed(app, closing);
}

/// The user closes every window, as `close` does. Entries of other kinds stay.
pub fn close_all(app: &AppHandle) {
    let closing = registry().mark_closing_where(|_| true);
    forget_closed(app, closing);
}

/// Picks the windows showing `path`, compared as saved image paths are (`attachments::normalize`).
fn same_image(path: &str) -> impl Fn(&Live) -> bool {
    let wanted = crate::attachments::normalize(path);
    move |live| wanted.is_some() && crate::attachments::normalize(&live.path) == wanted
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

/// Zooms the window `label` so its image is `width` CSS pixels wide, keeping its proportions,
/// within `floats::zoom_rect`'s bounds on the monitor that shows it, around the same center.
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
    reserve(epoch, &new_id(), "a.png", 1.0, folder).unwrap().0
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
        let (label, _) = registry
            .reserve(epoch, &new_id(), "a.png", 1.0, &folder())
            .unwrap();
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
            assert!(!is_live(label), "{label}");
        }
        let epoch = registry().epoch;
        let (label, _) = reserve(epoch, &new_id(), "a.png", 1.0, &folder()).unwrap();
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
        assert!(!show_if_current(&registry, &label, epoch, |_| panic!(
            "shown"
        )));
        let reserved = lock(&registry).reserve(epoch, &new_id(), "b.png", 1.0, &folder());
        assert_eq!(reserved.err(), Some(Error::FolderChanged));
        let epoch = lock(&registry).epoch;
        assert!(
            lock(&registry)
                .reserve(epoch, &new_id(), "b.png", 1.0, &folder())
                .is_ok()
        );
    }

    #[test]
    fn at_most_eight_windows_open() {
        let mut registry = Registry::new();
        for _ in 0..MAX_FLOATING {
            assert!(
                registry
                    .reserve(0, &new_id(), "a.png", 1.0, &folder())
                    .is_ok()
            );
        }
        let refused = registry.reserve(0, &new_id(), "a.png", 1.0, &folder());
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
                // As `close`: the window leaves the registry, then its entry under `SAVING`.
                lock(&registry).take(&label);
                store.borrow_mut().remove(&[&id]);
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
                });
                // Still under `SAVING`: a close's own removal waits for it, so it lands after this.
                if after_save {
                    interrupt();
                }
                Ok(())
            },
            |_| shown = true,
        );
        assert_eq!(finished.is_ok(), shown);
        (store.borrow().entries().count(), shown)
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
            |raw| shown_saved = raw == 7 && *saved.borrow(),
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
        let reserved = registry.reserve(during, &new_id(), "a.png", 1.0, &folder());
        assert_eq!(reserved.err(), Some(Error::FolderChanged));
        assert!(registry.end_switch());
        assert!(!registry.current(epoch));
        assert!(!registry.current(during));
        let after = registry.epoch;
        assert!(
            registry
                .reserve(after, &new_id(), "a.png", 1.0, &folder())
                .is_ok()
        );
    }

    // Regression: a folder choice that revoked a window the user was closing kept its entry, so
    // the closed window came back with that folder.
    #[test]
    fn a_close_overtaken_by_a_folder_change_is_still_forgotten() {
        let mut registry = Registry::new();
        let mut store = floats::Store::default();
        for path in ["a.png", "b.png"] {
            let id = new_id();
            registry.reserve(0, &id, path, 1.0, &folder()).unwrap();
            store.add(Entry {
                id,
                kind: Kind::Image,
                path: path.into(),
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                opacity: 1.0,
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
            registry
                .reserve(0, &new_id(), path, 1.0, &folder())
                .unwrap();
        }
        let closing = registry.mark_closing_where(same_image("./attachments/MAP.png"));
        let labels: Vec<_> = closing.iter().map(|closed| closed.label.as_str()).collect();
        assert_eq!(labels, ["float-1", "float-2"]);
        assert_eq!(registry.paths(), ["attachments/b.png"]);
        assert!(
            registry
                .mark_closing_where(same_image("../map.png"))
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
            registry
                .reserve(0, &entry.id, &entry.path, 1.0, &folder())
                .unwrap();
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
        assert!(zoom(&label, 400.0).is_err());
        take(&label);
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
}
