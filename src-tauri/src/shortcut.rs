//! Global shortcut that opens and closes the panel. It stays in Rust, granting no permission to the UI.
//! The user may record another combination; the rules for one are pure and the registration is a
//! thin layer behind `Hotkeys`.

use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::mpsc::Sender;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_global_shortcut::{
    Builder, Code, GlobalShortcut, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

use crate::dock::Request;
use crate::error::Error;

pub const DEFAULT: &str = "Ctrl+Alt+Space";

/// A key combination: the modifiers held and the physical key as a W3C `code`, such as `KeyA`.
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

impl Combination {
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
#[derive(Default)]
struct State {
    current: Option<Combination>,
    registered: bool,
    failed: Option<String>,
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

    /// Unregisters while recording, so pressing the current combination reaches the panel.
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
        }
    }
}

/// What the UI shows: the active combination, if any, and one that could not be registered.
#[derive(Serialize)]
pub struct Status {
    shortcut: Option<String>,
    failed: Option<String>,
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

pub fn status(app: &AppHandle) -> Status {
    with_state(app, |state, _| state.status())
}

/// While recording, the current combination is released so the panel receives it.
pub fn record(app: &AppHandle, recording: bool) -> Status {
    with_state(app, |state, keys| {
        if recording {
            state.release(keys);
        } else {
            state.resume(keys);
        }
        state.status()
    })
}

/// Ends recording with `pressed`. A failed save keeps the new combination for this session.
pub fn change(app: &AppHandle, pressed: Combination) -> Result<Status, Error> {
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
    fn keys_whose_name_depends_on_the_layout_are_unsupported() {
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
