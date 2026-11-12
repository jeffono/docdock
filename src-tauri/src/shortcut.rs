//! Global shortcut that opens and closes the panel. It stays in Rust, granting no permission to the UI.
//! The user may record another combination; the rules for one are pure and the registration is a
//! thin layer behind `Hotkeys`.

use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{
    Builder, Code, GlobalShortcut, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

use crate::dock::Request;
use crate::error::Error;

pub const DEFAULT: &str = "Ctrl+Alt+Space";

/// A recording nobody ends, such as after the UI reloads, gives the shortcut back after this long.
const RECORDING_LIMIT: Duration = Duration::from_secs(30);

/// A key combination: the modifiers held and the key as a W3C `code`, such as `KeyA`. Registration
/// reads `KeyA` as the virtual key A, which follows the layout; `on_layout` translates a recorded
/// physical position to it.
#[derive(Debug, PartialEq, Clone, Deserialize)]
pub struct Combination {
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
    code: String,
}

/// The shown name of a supported key and whether it types a character.
fn key(code: &str) -> Option<(String, bool)> {
    let single = |rest: &str, valid: fn(&char) -> bool| {
        let mut chars = rest.chars();
        chars
            .next()
            .filter(valid)
            .filter(|_| chars.next().is_none())
    };
    if let Some(letter) = code
        .strip_prefix("Key")
        .and_then(|rest| single(rest, char::is_ascii_uppercase))
    {
        return Some((letter.to_string(), true));
    }
    if let Some(digit) = code
        .strip_prefix("Digit")
        .and_then(|rest| single(rest, char::is_ascii_digit))
    {
        return Some((digit.to_string(), true));
    }
    if let Some(arrow) = code.strip_prefix("Arrow")
        && ["Up", "Down", "Left", "Right"].contains(&arrow)
    {
        return Some((arrow.to_owned(), false));
    }
    let function = code
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n) && code == format!("F{n}"));
    let named = [
        "Space", "Home", "End", "PageUp", "PageDown", "Insert", "Delete",
    ];
    (function || named.contains(&code)).then(|| (code.to_owned(), false))
}

/// The `code` of a shown key name, the inverse of `key`.
fn code(name: &str) -> String {
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_uppercase() => format!("Key{c}"),
        (Some(c), None) if c.is_ascii_digit() => format!("Digit{c}"),
        _ if ["Up", "Down", "Left", "Right"].contains(&name) => format!("Arrow{name}"),
        _ => name.to_owned(),
    }
}

/// Scan codes of the keys whose virtual key depends on the layout, named by their US position.
const LAYOUT_KEYS: [(&str, u32); 49] = [
    ("Digit1", 0x02),
    ("Digit2", 0x03),
    ("Digit3", 0x04),
    ("Digit4", 0x05),
    ("Digit5", 0x06),
    ("Digit6", 0x07),
    ("Digit7", 0x08),
    ("Digit8", 0x09),
    ("Digit9", 0x0A),
    ("Digit0", 0x0B),
    ("Minus", 0x0C),
    ("Equal", 0x0D),
    ("KeyQ", 0x10),
    ("KeyW", 0x11),
    ("KeyE", 0x12),
    ("KeyR", 0x13),
    ("KeyT", 0x14),
    ("KeyY", 0x15),
    ("KeyU", 0x16),
    ("KeyI", 0x17),
    ("KeyO", 0x18),
    ("KeyP", 0x19),
    ("BracketLeft", 0x1A),
    ("BracketRight", 0x1B),
    ("KeyA", 0x1E),
    ("KeyS", 0x1F),
    ("KeyD", 0x20),
    ("KeyF", 0x21),
    ("KeyG", 0x22),
    ("KeyH", 0x23),
    ("KeyJ", 0x24),
    ("KeyK", 0x25),
    ("KeyL", 0x26),
    ("Semicolon", 0x27),
    ("Quote", 0x28),
    ("Backquote", 0x29),
    ("Backslash", 0x2B),
    ("KeyZ", 0x2C),
    ("KeyX", 0x2D),
    ("KeyC", 0x2E),
    ("KeyV", 0x2F),
    ("KeyB", 0x30),
    ("KeyN", 0x31),
    ("KeyM", 0x32),
    ("Comma", 0x33),
    ("Period", 0x34),
    ("Slash", 0x35),
    ("IntlBackslash", 0x56),
    ("IntlRo", 0x73),
];

impl Combination {
    /// Translates a physical position to the key the layout gives it, so the combination shown and
    /// registered is the one pressed: on AZERTY the `KeyQ` position is A. A position whose virtual
    /// key is not a letter or digit becomes unsupported. `virtual_key` maps a scan code to it.
    pub fn on_layout(mut self, virtual_key: impl Fn(u32) -> u32) -> Self {
        if let Some(&(_, scan)) = LAYOUT_KEYS.iter().find(|(code, _)| *code == self.code) {
            self.code = match char::from_u32(virtual_key(scan)) {
                Some(c @ 'A'..='Z') => format!("Key{c}"),
                Some(c @ '0'..='9') => format!("Digit{c}"),
                _ => "Unidentified".to_owned(),
            };
        }
        self
    }

    /// Refuses what would take over typing: a key without Ctrl, Alt or Win, and Ctrl+Alt with a
    /// character key, which is AltGr on layouts such as ABNT2.
    pub fn validate(self) -> Result<Self, Error> {
        let (_, character) = key(&self.code).ok_or(Error::ShortcutKeyUnsupported)?;
        if !(self.ctrl || self.alt || self.win) {
            return Err(Error::ShortcutModifierRequired);
        }
        if self.ctrl && self.alt && character {
            return Err(Error::ShortcutAltGr);
        }
        Ok(self)
    }

    /// Reads the shown form, such as `Ctrl+Alt+Space`, as saved in the settings.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let mut combination = Self {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            code: String::new(),
        };
        let (modifiers, name) = text.rsplit_once('+').unwrap_or(("", text));
        for modifier in modifiers.split('+').filter(|m| !m.is_empty()) {
            let held = match modifier {
                "Ctrl" => &mut combination.ctrl,
                "Alt" => &mut combination.alt,
                "Shift" => &mut combination.shift,
                "Win" => &mut combination.win,
                _ => return Err(Error::ShortcutKeyUnsupported),
            };
            *held = true;
        }
        combination.code = code(name);
        combination.validate()
    }

    fn shortcut(&self) -> Option<Shortcut> {
        let held = [
            (self.ctrl, Modifiers::CONTROL),
            (self.alt, Modifiers::ALT),
            (self.shift, Modifiers::SHIFT),
            (self.win, Modifiers::SUPER),
        ];
        let modifiers = held
            .into_iter()
            .filter(|(on, _)| *on)
            .fold(Modifiers::empty(), |all, (_, m)| all | m);
        Some(Shortcut::new(
            Some(modifiers),
            Code::from_str(&self.code).ok()?,
        ))
    }
}

/// The shown form, with modifiers in Windows order: `Win+Ctrl+Alt+Shift+Key`.
impl fmt::Display for Combination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let held = [
            (self.win, "Win"),
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
        ];
        for (_, name) in held.into_iter().filter(|(on, _)| *on) {
            write!(f, "{name}+")?;
        }
        let name = key(&self.code).map_or_else(|| self.code.clone(), |(name, _)| name);
        f.write_str(&name)
    }
}

/// The system's global hotkeys; tests use a fake.
trait Hotkeys {
    /// False if the combination is unavailable, such as when another app holds it.
    fn register(&mut self, combination: &Combination) -> bool;
    fn unregister(&mut self, combination: &Combination);
}

/// The activation shortcut, `failed` being a saved or previous one that could not be registered.
/// `session` counts recordings, so a timer only ends the one it was started for.
#[derive(Default)]
struct State {
    current: Option<Combination>,
    registered: bool,
    failed: Option<String>,
    recording: bool,
    session: u64,
}

impl State {
    /// The saved combination, else the default; a saved one that fails is reported in `failed`.
    fn start(saved: Option<&str>, keys: &mut impl Hotkeys) -> Self {
        let mut state = Self::default();
        let failed = saved.filter(|text| state.change(Combination::parse(text), keys).is_err());
        if (saved.is_none() || failed.is_some())
            && let Err(error) = state.change(Combination::parse(DEFAULT), keys)
        {
            eprintln!("[shortcut] failed to register {DEFAULT}: {error:?}");
        }
        state.failed = failed.map(str::to_owned);
        state
    }

    /// Replaces the current combination; if `new` is refused or unavailable, the current one stays.
    fn change(
        &mut self,
        new: Result<Combination, Error>,
        keys: &mut impl Hotkeys,
    ) -> Result<(), Error> {
        self.recording = false;
        let result = new.and_then(|new| {
            self.release(keys);
            if keys.register(&new) {
                Ok(new)
            } else {
                Err(Error::ShortcutTaken {
                    shortcut: new.to_string(),
                })
            }
        });
        match result {
            Ok(new) => {
                self.current = Some(new);
                self.registered = true;
                self.failed = None;
                Ok(())
            }
            Err(error) => {
                self.resume(keys);
                Err(error)
            }
        }
    }

    /// Releases the current combination so pressing it reaches the panel; returns the session.
    fn start_recording(&mut self, keys: &mut impl Hotkeys) -> u64 {
        self.release(keys);
        self.recording = true;
        self.session += 1;
        self.session
    }

    /// Registers the current combination again, recording or not; answers whether it was.
    fn stop_recording(&mut self, keys: &mut impl Hotkeys) -> bool {
        let was = self.recording;
        self.recording = false;
        self.resume(keys);
        was
    }

    /// Ends an abandoned recording, unless another one started since.
    fn expire(&mut self, session: u64, keys: &mut impl Hotkeys) -> bool {
        self.recording && self.session == session && self.stop_recording(keys)
    }

    fn release(&mut self, keys: &mut impl Hotkeys) {
        if let Some(current) = self.current.as_ref().filter(|_| self.registered) {
            keys.unregister(current);
        }
        self.registered = false;
    }

    /// Registers the current combination again; if another app took it meanwhile, none is left.
    fn resume(&mut self, keys: &mut impl Hotkeys) {
        let Some(current) = self.current.as_ref().filter(|_| !self.registered) else {
            return;
        };
        if keys.register(current) {
            self.registered = true;
        } else {
            self.failed = Some(current.to_string());
            self.current = None;
        }
    }

    fn status(&self) -> Status {
        Status {
            shortcut: self.current.as_ref().map(Combination::to_string),
            failed: self.failed.clone(),
            recording: self.recording,
        }
    }
}

/// What the UI shows: the active combination, if any, one that could not be registered, and
/// whether a recording holds it released.
#[derive(Serialize, Clone)]
pub struct Status {
    shortcut: Option<String>,
    failed: Option<String>,
    recording: bool,
}

struct Activation(Mutex<State>);

struct Global<'a>(&'a AppHandle);

impl Hotkeys for Global<'_> {
    fn register(&mut self, combination: &Combination) -> bool {
        let Some(manager) = self.0.try_state::<GlobalShortcut<Wry>>() else {
            return false;
        };
        let Some(shortcut) = combination.shortcut() else {
            return false;
        };
        manager
            .register(shortcut)
            .inspect_err(|error| eprintln!("[shortcut] failed to register {combination}: {error}"))
            .is_ok()
    }

    fn unregister(&mut self, combination: &Combination) {
        if let Some(shortcut) = combination.shortcut()
            && let Err(error) = self.0.global_shortcut().unregister(shortcut)
        {
            eprintln!("[shortcut] failed to unregister {combination}: {error}");
        }
    }
}

/// If no shortcut is available, such as when another app uses it, the cursor can still open the panel.
pub fn setup(app: &tauri::App, requests: Sender<Request>) {
    let plugin = Builder::new()
        .with_handler(move |_, _, event| {
            if event.state() == ShortcutState::Pressed {
                // This only fails if the panel thread has stopped.
                let _ = requests.send(Request::Shortcut);
            }
        })
        .build();
    if let Err(error) = app.handle().plugin(plugin) {
        eprintln!("[shortcut] failed to start global shortcuts: {error}");
    }
    let saved = crate::notes::activation_shortcut(app.handle());
    let state = State::start(saved.as_deref(), &mut Global(app.handle()));
    app.manage(Activation(Mutex::new(state)));
}

fn with_state<T>(app: &AppHandle, work: impl FnOnce(&mut State, &mut Global) -> T) -> T {
    let activation = app.state::<Activation>();
    let mut state = activation.0.lock().unwrap_or_else(|e| e.into_inner());
    work(&mut state, &mut Global(app))
}

/// While recording, the current combination is released so the panel receives it. Rust ends a
/// recording the UI abandons: when the panel loses focus or hides, and after `RECORDING_LIMIT`.
/// Stopping also serves to read the state: a reloaded UI was not recording.
pub fn record(app: &AppHandle, recording: bool) -> Status {
    let (status, session) = with_state(app, |state, keys| {
        let session = if recording {
            Some(state.start_recording(keys))
        } else {
            state.stop_recording(keys);
            None
        };
        (state.status(), session)
    });
    if let Some(session) = session {
        let app = app.clone();
        thread::spawn(move || {
            thread::sleep(RECORDING_LIMIT);
            on_main_thread(&app, move |state, keys| state.expire(session, keys));
        });
    }
    status
}

/// Ends any recording; callable from any thread, such as the panel's.
pub fn end_recording(app: &AppHandle) {
    on_main_thread(app, |state, keys| state.stop_recording(keys));
}

/// Hotkeys register on the main thread, which also runs the commands, so the state is only
/// touched there. Tells the UI when `work` ended a recording.
fn on_main_thread(
    app: &AppHandle,
    work: impl FnOnce(&mut State, &mut Global) -> bool + Send + 'static,
) {
    let handle = app.clone();
    let result = app.run_on_main_thread(move || {
        if handle.try_state::<Activation>().is_none() {
            return;
        }
        let (ended, status) =
            with_state(&handle, |state, keys| (work(state, keys), state.status()));
        if ended {
            let _ = handle.emit_to("main", "activation", status);
        }
    });
    if let Err(error) = result {
        eprintln!("[shortcut] failed to reach the main thread: {error}");
    }
}

/// Ends recording with `pressed`. A failed save keeps the new combination for this session.
pub fn change(app: &AppHandle, pressed: Combination) -> Result<Status, Error> {
    let pressed = pressed.on_layout(crate::platform::keyboard::virtual_key);
    let status = with_state(app, |state, keys| {
        state.change(pressed.validate(), keys)?;
        Ok::<_, Error>(state.status())
    })?;
    crate::notes::set_activation_shortcut(app, status.shortcut.clone())?;
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressed(modifiers: &str, code: &str) -> Combination {
        Combination {
            ctrl: modifiers.contains('c'),
            alt: modifiers.contains('a'),
            shift: modifiers.contains('s'),
            win: modifiers.contains('w'),
            code: code.to_owned(),
        }
    }

    /// Registers anything but `taken`, tracking what is registered.
    #[derive(Default)]
    struct Fake {
        taken: Vec<String>,
        registered: Vec<String>,
    }
    impl Hotkeys for Fake {
        fn register(&mut self, combination: &Combination) -> bool {
            let name = combination.to_string();
            let free = !self.taken.contains(&name);
            if free {
                self.registered.push(name);
            }
            free
        }
        fn unregister(&mut self, combination: &Combination) {
            self.registered
                .retain(|name| *name != combination.to_string());
        }
    }
    fn taken(names: &[&str]) -> Fake {
        Fake {
            taken: names.iter().map(|n| n.to_string()).collect(),
            ..Fake::default()
        }
    }

    #[test]
    fn a_key_needs_ctrl_alt_or_win() {
        for modifiers in ["", "s"] {
            assert_eq!(
                pressed(modifiers, "KeyA").validate(),
                Err(Error::ShortcutModifierRequired)
            );
        }
        for modifiers in ["c", "a", "w"] {
            assert!(pressed(modifiers, "KeyA").validate().is_ok());
        }
    }

    #[test]
    fn ctrl_alt_with_a_character_key_is_refused_as_altgr() {
        for (modifiers, code) in [("ca", "KeyQ"), ("cas", "KeyE"), ("caw", "Digit2")] {
            assert_eq!(
                pressed(modifiers, code).validate(),
                Err(Error::ShortcutAltGr)
            );
        }
        for code in ["Space", "F5", "ArrowUp", "Home"] {
            assert!(pressed("ca", code).validate().is_ok());
        }
    }

    #[test]
    fn other_keys_are_unsupported() {
        for code in [
            "Semicolon",
            "Backquote",
            "Escape",
            "Tab",
            "F25",
            "F05",
            "KeyAB",
            "Numpad1",
        ] {
            assert_eq!(
                pressed("c", code).validate(),
                Err(Error::ShortcutKeyUnsupported)
            );
        }
    }

    #[test]
    fn the_saved_form_reads_back_as_recorded() {
        for (modifiers, code, text) in [
            ("ca", "Space", "Ctrl+Alt+Space"),
            ("cs", "KeyK", "Ctrl+Shift+K"),
            ("wa", "Digit7", "Win+Alt+7"),
            ("as", "ArrowLeft", "Alt+Shift+Left"),
            ("c", "F12", "Ctrl+F12"),
            ("c", "PageDown", "Ctrl+PageDown"),
        ] {
            let combination = pressed(modifiers, code);
            assert_eq!(combination.to_string(), text);
            assert_eq!(Combination::parse(text), Ok(combination));
        }
        assert!(Combination::parse("Hyper+K").is_err());
        assert!(Combination::parse("").is_err());
    }

    /// AZERTY: A and Q swap, the `Semicolon` position is M, `KeyM` is a comma and digits stay.
    fn azerty(scan: u32) -> u32 {
        match scan {
            0x10 => 'A' as u32,
            0x1E => 'Q' as u32,
            0x27 => 'M' as u32,
            0x32 => 0xBC,
            0x03 => '2' as u32,
            _ => 0,
        }
    }

    #[test]
    fn letters_and_digits_follow_the_layout() {
        for (code, key) in [
            ("KeyQ", "KeyA"),
            ("KeyA", "KeyQ"),
            ("Semicolon", "KeyM"),
            ("Digit2", "Digit2"),
            ("F5", "F5"),
            ("Space", "Space"),
        ] {
            assert_eq!(pressed("cs", code).on_layout(azerty).code, key);
        }
        assert_eq!(
            pressed("cs", "KeyM").on_layout(azerty).validate(),
            Err(Error::ShortcutKeyUnsupported)
        );
    }

    #[test]
    fn the_altgr_rule_applies_to_the_key_the_layout_gives() {
        for code in ["Semicolon", "Digit2"] {
            assert_eq!(
                pressed("ca", code).on_layout(azerty).validate(),
                Err(Error::ShortcutAltGr)
            );
        }
    }

    #[test]
    fn an_abandoned_recording_registers_the_previous_shortcut_when_it_expires() {
        let mut keys = Fake::default();
        let mut state = State::start(None, &mut keys);
        let session = state.start_recording(&mut keys);
        assert!(keys.registered.is_empty());
        assert!(state.expire(session, &mut keys));
        assert_eq!(keys.registered, [DEFAULT]);
        assert!(!state.status().recording);
    }

    #[test]
    fn an_earlier_timer_leaves_a_later_recording_alone() {
        let mut keys = Fake::default();
        let mut state = State::start(None, &mut keys);
        let first = state.start_recording(&mut keys);
        state.stop_recording(&mut keys);
        state.start_recording(&mut keys);
        assert!(!state.expire(first, &mut keys));
        assert!(keys.registered.is_empty());
    }

    #[test]
    fn without_a_saved_shortcut_the_default_starts() {
        let mut keys = Fake::default();
        let state = State::start(None, &mut keys);
        assert_eq!(keys.registered, [DEFAULT]);
        assert_eq!(state.failed, None);
    }

    #[test]
    fn a_saved_shortcut_that_fails_falls_back_to_the_default_with_a_notice() {
        for (saved, mut keys) in [
            ("Ctrl+Shift+F9", taken(&["Ctrl+Shift+F9"])),
            ("Ctrl+Alt+Q", Fake::default()),
        ] {
            let state = State::start(Some(saved), &mut keys);
            assert_eq!(keys.registered, [DEFAULT]);
            assert_eq!(state.status().shortcut.as_deref(), Some(DEFAULT));
            assert_eq!(state.failed.as_deref(), Some(saved));
        }
    }

    #[test]
    fn a_taken_default_leaves_no_shortcut() {
        let mut keys = taken(&[DEFAULT]);
        let state = State::start(None, &mut keys);
        assert!(keys.registered.is_empty());
        assert_eq!(state.status().shortcut, None);
    }

    #[test]
    fn a_taken_combination_keeps_the_previous_one_after_recording() {
        let mut keys = taken(&["Ctrl+Shift+F9"]);
        let mut state = State::start(None, &mut keys);
        state.release(&mut keys);
        assert!(keys.registered.is_empty());
        let result = state.change(Ok(pressed("cs", "F9")), &mut keys);
        assert_eq!(
            result,
            Err(Error::ShortcutTaken {
                shortcut: "Ctrl+Shift+F9".into()
            })
        );
        assert_eq!(keys.registered, [DEFAULT]);
        assert_eq!(state.status().shortcut.as_deref(), Some(DEFAULT));
    }

    #[test]
    fn a_refused_combination_keeps_the_previous_one_registered() {
        let mut keys = Fake::default();
        let mut state = State::start(None, &mut keys);
        state.release(&mut keys);
        let result = state.change(pressed("ca", "KeyQ").validate(), &mut keys);
        assert_eq!(result, Err(Error::ShortcutAltGr));
        assert_eq!(keys.registered, [DEFAULT]);
    }

    #[test]
    fn a_new_combination_replaces_the_previous_and_clears_the_notice() {
        let mut keys = taken(&["Ctrl+Shift+F9"]);
        let mut state = State::start(Some("Ctrl+Shift+F9"), &mut keys);
        state.change(Ok(pressed("w", "F8")), &mut keys).unwrap();
        assert_eq!(keys.registered, ["Win+F8"]);
        assert_eq!(state.failed, None);
    }
}
