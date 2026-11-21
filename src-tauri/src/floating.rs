//! Floating windows: note images detached from the panel into borderless, topmost windows that
//! never activate, with their own opacity and a click-through mode, reopened where they were from
//! `.docdock/floating.json`. The rules are pure in `floats.rs`; this runs their effects. Each window
//! is a secondary app window ([0014](../../docs/decisions/0014-app-windows.md)) labeled `float-<n>`
//! for this run: it has no capability, reaches only the `float_*` commands, and only for itself,
//! and hears what to show only through the IPC channel it hands over with `float_state`.
//!
//! One worker thread saves moves and fades after a pause and, while any window lets clicks
//! through, watches Alt and the cursor so holding Alt over such a window reaches its controls.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::dock::Request;
use crate::error::Error;
use crate::floats::{self, Entry, Kind, MAX_FLOATING};
use crate::i18n::Locale;
use crate::platform::{self, Rect};

/// How often Alt and the cursor are read while a window lets clicks through.
const POLL: Duration = Duration::from_millis(50);
/// Moves and fades are saved once they pause this long.
const SAVE_DELAY: Duration = Duration::from_millis(500);
/// How long exiting waits for the last moves to be saved.
const FLUSH_WAIT: Duration = Duration::from_secs(1);
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
    /// `notes::folder_choices` when it opened: it is saved only into that folder.
    folder: u64,
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

struct Registry {
    /// The last label number given; labels are never reused within a run.
    next: u64,
    live: Vec<Live>,
}

/// The floating windows of this run. Never taken on the main thread: its holders call Win32
/// functions that wait for the main thread, such as changing a window's style. Commands that take
/// it are asynchronous for that reason.
static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    next: 0,
    live: Vec::new(),
});
/// The labels of `REGISTRY`'s windows, for the command barrier and `note-asset`, which run on the
/// main thread. Held only to read or change the set, never around anything else.
static LABELS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
static WORKER: OnceLock<Sender<Message>> = OnceLock::new();
/// One restore at a time, so two never open the same saved window.
static RESTORING: Mutex<()> = Mutex::new(());

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
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
pub fn admit(label: &str) {
    labels().insert(label.to_owned());
}

fn forget_label(label: &str) {
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
    folder: u64,
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
                        folder: live.folder,
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
            Message::Flush(done) => {
                self.save();
                let _ = done.send(());
            }
        }
    }

    /// Saves the moves and fades waiting, each into the folder its window opened on: a window
    /// closed meanwhile, or one of another folder, changes nothing.
    fn save(&mut self) {
        self.save_at = None;
        let folder = crate::notes::folder_choices();
        let dirty: Vec<_> = std::mem::take(&mut self.dirty)
            .into_iter()
            .filter(|(_, dirty)| dirty.folder == folder)
            .collect();
        if dirty.is_empty() {
            return;
        }
        let saved = crate::notes::update_floating(&self.app, |store| {
            for (id, dirty) in &dirty {
                store.update(id, dirty.rect, dirty.opacity);
            }
        });
        if let Err(error) = saved {
            eprintln!("[floating] position not saved: {error:?}");
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
fn report(app: &AppHandle) {
    let any = registry().live.iter().any(|live| live.click_through);
    crate::tray::set_unlock_enabled(app, any);
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

/// Takes a label and a place in the registry for a window still to be built; answers its label
/// and how many windows were open already.
fn reserve(id: &str, path: &str, opacity: f64, folder: u64) -> Result<(String, usize), Error> {
    let mut registry = registry();
    let open = registry.live.len();
    if open >= MAX_FLOATING {
        return Err(Error::FloatingLimit { max: MAX_FLOATING });
    }
    registry.next += 1;
    let label = floats::label(registry.next);
    admit(&label);
    registry.live.push(Live {
        label: label.clone(),
        id: id.to_owned(),
        path: path.to_owned(),
        raw: 0,
        folder,
        opacity: floats::clamp_opacity(opacity),
        click_through: false,
        revealed: false,
        version: 1,
        channel: None,
    });
    Ok((label, open))
}

/// Removes the window `label` from the registry, if it is still there.
fn take(label: &str) -> Option<Live> {
    forget_label(label);
    let mut registry = registry();
    let at = registry.live.iter().position(|live| live.label == label)?;
    Some(registry.live.remove(at))
}

fn destroy(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label)
        && let Err(error) = window.destroy()
    {
        eprintln!("[floating] window not destroyed: {error}");
    }
}

/// Builds the reserved window `label`, hardened as every secondary app window, and shows it at
/// `rect` without activating it. Gives up, destroying it, if the registry no longer holds it, as
/// after the notes folder went away.
fn build(app: &AppHandle, label: &str, rect: Rect) -> Result<(), Error> {
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
    if with_live(label, |live| live.raw = raw).is_err() {
        let _ = window.destroy();
        return Err(Error::unexpected("floating window closed while opening"));
    }
    // Twice: a window landing on a monitor of another scale is resized by the DPI change.
    platform::show_topmost(raw, rect);
    platform::show_topmost(raw, rect);
    Ok(())
}

/// Opens a window for the saved `entry` at `rect`, for the folder counted `folder`.
fn open_entry(app: &AppHandle, entry: &Entry, rect: Rect, folder: u64) -> Result<(), Error> {
    let (label, _) = reserve(&entry.id, &entry.path, entry.opacity, folder)?;
    let built = build(app, &label, rect);
    if built.is_err() {
        take(&label);
    }
    built
}

/// Detaches `path`, an image relative to the notes folder, into a floating window where the side
/// preview would show it for its thumbnail at `thumb` (a CSS box in `main`), slightly apart from
/// the windows already open, and saves it.
pub fn open(
    app: &AppHandle,
    requests: &Sender<Request>,
    path: String,
    thumb: [i32; 4],
    natural: (u32, u32),
) -> Result<(), Error> {
    let folder = crate::notes::check_image(app, &path)?;
    // A file another version wrote is never overwritten, so the window could not be saved.
    crate::notes::read_floating(app)?.ok_or(Error::FloatingVersion)?;
    let id = new_id();
    let (label, open) = reserve(&id, &path, 1.0, folder)?;
    let opened = place(requests, thumb, natural).and_then(|(rect, dpi)| {
        let rect = floats::cascade(rect, open, dpi);
        let rect = floats::fit_to_monitors(rect, &platform::monitors());
        let mut entry = Entry {
            id,
            kind: Kind::Image,
            path,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            opacity: 1.0,
        };
        entry.set_rect(rect);
        build(app, &label, rect)?;
        if crate::notes::folder_choices() != folder {
            return Err(Error::FolderChanged);
        }
        crate::notes::update_floating(app, |store| store.add(entry))
    });
    if opened.is_err() && take(&label).is_some() {
        destroy(app, &label);
    }
    opened
}

/// Where the panel thread places a window for the thumbnail, and its monitor's DPI.
fn place(
    requests: &Sender<Request>,
    thumb: [i32; 4],
    natural: (u32, u32),
) -> Result<(Rect, u32), Error> {
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

/// Reopens the windows saved in the notes folder, unlocked, where they were as far as the
/// monitors there are now allow. Entries whose image is gone are dropped; those whose image cannot
/// be checked now stay for the next time. Runs off the main thread.
pub fn restore(app: &AppHandle) {
    let _restoring = RESTORING.lock().unwrap_or_else(|e| e.into_inner());
    let folder = crate::notes::folder_choices();
    let store = match crate::notes::read_floating(app) {
        Ok(Some(store)) => store,
        Ok(None) => return,
        Err(error) => {
            eprintln!("[floating] saved windows not read: {error:?}");
            return;
        }
    };
    let monitors = platform::monitors();
    let mut gone = Vec::new();
    for entry in store.entries() {
        if registry().live.iter().any(|live| live.id == entry.id) {
            continue;
        }
        match crate::notes::image_present(app, &entry.path) {
            Ok(true) => {}
            Ok(false) => {
                gone.push(entry.id.as_str());
                continue;
            }
            Err(_) => continue,
        }
        let rect = floats::fit_to_monitors(entry.rect(), &monitors);
        match open_entry(app, entry, rect, folder) {
            Ok(()) => {}
            Err(Error::FloatingLimit { .. }) => break,
            Err(error) => eprintln!("[floating] window not reopened: {error:?}"),
        }
    }
    if !gone.is_empty()
        && let Err(error) = crate::notes::update_floating(app, |store| store.remove(&gone))
    {
        eprintln!("[floating] gone images not dropped: {error:?}");
    }
}

/// The notes folder went away, such as a vault that locked: every window goes at once, and its
/// saved entry stays for when the folder returns ([0009](../../docs/decisions/0009-note-privacy.md)).
pub fn folder_left(app: &AppHandle) {
    let gone = std::mem::take(&mut registry().live);
    for live in &gone {
        forget_label(&live.label);
        destroy(app, &live.label);
    }
    report(app);
}

/// The notes folder came back: its windows reopen.
pub fn folder_returned(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || restore(&app));
}

/// Another notes folder was chosen, or the one at the path confirmed: the windows of the folder
/// before close, and the new folder's reopen.
pub fn folder_chosen(app: &AppHandle) {
    folder_left(app);
    folder_returned(app);
}

/// The user closed `label`: its window and its saved entry go.
pub fn close(app: &AppHandle, label: &str) {
    let Some(live) = take(label) else {
        return;
    };
    destroy(app, label);
    if live.folder == crate::notes::folder_choices()
        && let Err(error) = crate::notes::update_floating(app, |store| store.remove(&[&live.id]))
    {
        eprintln!("[floating] closed window not forgotten: {error:?}");
    }
    report(app);
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
    with_live(label, |live| {
        if live.raw != 0 {
            platform::set_click_through(live.raw, through);
        }
        live.click_through = through;
        live.revealed = false;
        live.changed();
    })?;
    send(Message::Wake);
    report(app);
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

/// Saves the moves and fades still waiting, before the app exits. Safe on the main thread: it
/// waits for the worker only so long.
pub fn flush() {
    let Some(worker) = WORKER.get() else {
        return;
    };
    let (done, finished) = mpsc::channel();
    if worker.send(Message::Flush(done)).is_ok() {
        let _ = finished.recv_timeout(FLUSH_WAIT);
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

#[cfg(test)]
mod tests {
    use super::*;

    // Catches the image preview, a closed window or another window's label reaching a floating
    // window's commands.
    #[test]
    fn only_a_live_floating_window_reaches_its_commands() {
        for label in ["image-preview", "main", "web", "float-424242"] {
            assert!(with_live(label, |_| ()).is_err(), "{label}");
            assert!(!is_live(label), "{label}");
        }
        let (label, _) = reserve(&new_id(), "a.png", 1.0, 0).unwrap();
        assert!(is_live(&label));
        assert!(with_live(&label, |_| ()).is_ok());
        take(&label);
        assert!(!is_live(&label));
        assert!(with_live(&label, |_| ()).is_err());
    }
}
