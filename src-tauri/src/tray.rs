//! Tray: its icon, and the screen side and autostart controls it shares with Preferences.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use tauri::image::Image;
use tauri::include_image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::dock::{Request, Side};
use crate::i18n::Locale;
use crate::notes;
use crate::platform::{autostart, taskbar};

struct SideItems([(Side, CheckMenuItem<Wry>); 2]);
struct AutostartItem(CheckMenuItem<Wry>);
struct UnlockItem(MenuItem<Wry>);
/// Asks for the glyph to be read again.
struct GlyphCheck(Sender<()>);

const TRAY_ID: &str = "docdock";

pub fn set_update_tooltip(app: &AppHandle, label: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID)
        && let Err(error) = tray.set_tooltip(Some(label))
    {
        eprintln!("[tray] failed to change the tooltip: {error}");
    }
}

/// `window` is a top-level window whose messages tell when the taskbar's theme or scale changes.
pub fn setup(
    app: &mut tauri::App,
    window: isize,
    locale: Locale,
    side: Side,
    requests: Sender<Request>,
) -> Result<(), Box<dyn std::error::Error>> {
    let [right, left, unlock, start_with_windows, quit] = match locale {
        Locale::En => [
            "Side: right",
            "Side: left",
            "Unlock floating windows",
            "Start with Windows",
            "Quit",
        ],
        Locale::PtBr => [
            "Lado: direito",
            "Lado: esquerdo",
            "Destravar janelas flutuantes",
            "Iniciar com o Windows",
            "Sair",
        ],
    };
    // An unreadable registry must not prevent the app from opening.
    if let Err(error) = autostart::repair() {
        eprintln!("[tray] failed to repair autostart: {error}");
    }
    let enabled = autostart::is_enabled().unwrap_or_else(|error| {
        eprintln!("[tray] failed to query autostart: {error}");
        false
    });
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        start_with_windows,
        true,
        enabled,
        None::<&str>,
    )?;
    app.manage(AutostartItem(autostart.clone()));
    let side_item = |id, label, value| {
        CheckMenuItem::with_id(app, id, label, true, side == value, None::<&str>)
            .map(|item| (value, item))
    };
    let sides = [
        side_item("side-right", right, Side::Right)?,
        side_item("side-left", left, Side::Left)?,
    ];
    app.manage(SideItems(sides.clone()));
    // Available while a floating window lets clicks through (`set_unlock_enabled`).
    let unlock = MenuItem::with_id(app, "unlock-floating", unlock, false, None::<&str>)?;
    app.manage(UnlockItem(unlock.clone()));
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", quit, true, None::<&str>)?;
    let update = crate::updates::menu_item(app, locale)?;
    let menu = Menu::with_items(
        app,
        &[
            &sides[0].1,
            &sides[1].1,
            &unlock,
            &separator,
            &autostart,
            &update,
            &quit,
        ],
    )?;
    let shown = glyph();
    let (changes, received) = mpsc::channel();
    let hover = changes.clone();
    app.manage(GlyphCheck(changes.clone()));
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(glyph_image(shown))
        .tooltip("DocDock")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "unlock-floating" => crate::floating::unlock_all(),
            "update" => crate::updates::request_exit(app, true),
            "side-right" | "side-left" => {
                let side = if event.id().as_ref() == "side-right" {
                    Side::Right
                } else {
                    Side::Left
                };
                if let Err(error) = select_side(app, side, &requests) {
                    eprintln!("[tray] failed to save side: {error:?}");
                }
            }
            "autostart" => {
                let result = match autostart::is_enabled() {
                    Ok(enabled) => select_autostart(app, !enabled),
                    Err(error) => {
                        // The menu already flipped its own check mark: put back the actual state.
                        show_autostart(app);
                        Err(crate::error::Error::unexpected(error))
                    }
                };
                if let Err(error) = result {
                    eprintln!("[tray] failed to change autostart: {error:?}");
                }
            }
            _ => {}
        })
        .on_tray_icon_event(move |_, event| {
            if let TrayIconEvent::Enter { .. } = event {
                let _ = hover.send(());
            }
        })
        .build(app)?;
    // Without this, the glyph keeps the theme and scale it started with.
    std::thread::spawn(move || follow_taskbar(&tray, shown, &received));
    if let Err(error) = taskbar::on_change(window, move || {
        let _ = changes.send(());
    }) {
        eprintln!("[tray] failed to follow the taskbar: {error}");
    }
    Ok(())
}

/// Glyph sizes in `icons/tray/`, ascending: the drawn ones plus 40 and 64 rendered by `pnpm icons`.
const SIZES: [u32; 8] = [16, 20, 24, 30, 32, 40, 48, 64];

/// A glyph: whether it is for a light taskbar, and its index in `SIZES`.
type Glyph = (bool, usize);

/// Index in `SIZES` of the glyph for a notification area that draws `needed`-pixel icons (16 at 100%
/// scale, 20 at 125%, 24 at 150%): that size, else the next larger one so the shell only scales
/// down, else the largest.
fn size_index(needed: u32) -> usize {
    SIZES
        .iter()
        .position(|&size| size >= needed)
        .unwrap_or(SIZES.len() - 1)
}

/// The glyph for the taskbar as it is now.
fn glyph() -> Glyph {
    (taskbar::light(), size_index(taskbar::small_icon_size()))
}

/// Glyphs in `SIZES` order: black for a light taskbar, white for a dark one. Embedded, so the tray
/// never depends on files beside the executable.
fn glyphs(light: bool) -> [Image<'static>; SIZES.len()] {
    if light {
        [
            include_image!("icons/tray/light-16.png"),
            include_image!("icons/tray/light-20.png"),
            include_image!("icons/tray/light-24.png"),
            include_image!("icons/tray/light-30.png"),
            include_image!("icons/tray/light-32.png"),
            include_image!("icons/tray/light-40.png"),
            include_image!("icons/tray/light-48.png"),
            include_image!("icons/tray/light-64.png"),
        ]
    } else {
        [
            include_image!("icons/tray/dark-16.png"),
            include_image!("icons/tray/dark-20.png"),
            include_image!("icons/tray/dark-24.png"),
            include_image!("icons/tray/dark-30.png"),
            include_image!("icons/tray/dark-32.png"),
            include_image!("icons/tray/dark-40.png"),
            include_image!("icons/tray/dark-48.png"),
            include_image!("icons/tray/dark-64.png"),
        ]
    }
}

fn glyph_image((light, index): Glyph) -> Image<'static> {
    glyphs(light)[index].clone()
}

/// How long settings messages must stop before the taskbar is read again.
const SETTLE: Duration = Duration::from_millis(250);

/// Reads the glyph again. Not every scale change on the taskbar's monitor reaches a top-level window
/// (an auto-hidden taskbar keeps the work area), so the glyph is also read when the user turns to the
/// app (the pointer enters the tray icon, the panel opens) instead of on a timer that would keep
/// waking an idle app.
pub fn check_glyph(app: &AppHandle) {
    if let Some(check) = app.try_state::<GlyphCheck>() {
        let _ = check.0.send(());
    }
}

/// Replaces the glyph when the taskbar's theme or scale changed. Messages come in bursts, mostly
/// about something else, and the shell may be the one broadcasting them: the glyph is read once they
/// settle, off the window's thread, and sent to the shell only if it differs.
fn follow_taskbar(tray: &TrayIcon, mut shown: Glyph, changes: &Receiver<()>) {
    while changes.recv().is_ok() {
        while changes.recv_timeout(SETTLE).is_ok() {}
        let glyph = glyph();
        if glyph == shown {
            continue;
        }
        match tray.set_icon(Some(glyph_image(glyph))) {
            Ok(()) => shown = glyph,
            Err(error) => eprintln!("[tray] failed to change icon: {error}"),
        }
    }
}

/// Enables "Unlock floating windows" while any floating window lets clicks through.
pub fn set_unlock_enabled(app: &AppHandle, enabled: bool) {
    if let Some(item) = app.try_state::<UnlockItem>()
        && let Err(error) = item.0.set_enabled(enabled)
    {
        eprintln!("[tray] failed to update the unlock item: {error}");
    }
}

pub fn autostart_enabled() -> Result<bool, crate::error::Error> {
    autostart::is_enabled().map_err(crate::error::Error::unexpected)
}

pub fn select_autostart(app: &AppHandle, enabled: bool) -> Result<(), crate::error::Error> {
    let saved = autostart::set_enabled(enabled).map_err(crate::error::Error::unexpected);
    // Even a partial failure (for example clearing StartupApproved) must show the actual state.
    show_autostart(app);
    saved
}

/// Shows the registry state in the tray and in an open Preferences view. Failures here only affect
/// what is displayed, so they are logged instead of failing the change.
fn show_autostart(app: &AppHandle) {
    let enabled = match autostart::is_enabled() {
        Ok(enabled) => enabled,
        Err(error) => return eprintln!("[tray] failed to query autostart: {error}"),
    };
    if let Err(error) = app.state::<AutostartItem>().0.set_checked(enabled) {
        eprintln!("[tray] failed to update check mark: {error}");
    }
    if let Err(error) = app.emit_to("main", "autostart-changed", enabled) {
        eprintln!("[tray] failed to report autostart: {error}");
    }
}

/// Saves the side and moves the panel right away. A failed save still moves it for this session.
pub fn select_side(
    app: &AppHandle,
    side: Side,
    requests: &Sender<Request>,
) -> Result<(), crate::error::Error> {
    let saved = notes::set_side(app, side);
    // Clicking toggles the item; the chosen side must stay checked and the other unchecked.
    for (value, item) in &app.state::<SideItems>().0 {
        if let Err(error) = item.set_checked(*value == side) {
            eprintln!("[tray] failed to update check mark: {error}");
        }
    }
    // This only fails if the panel thread has stopped.
    let _ = requests.send(Request::SetSide(side));
    saved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_index_selects_the_smallest_sufficient_glyph_or_the_largest() {
        for (needed, expected) in [
            (0, 16),
            (16, 16),
            (20, 20),
            (24, 24),
            (32, 32),
            (48, 48),
            (28, 30),
            (36, 40),
            (56, 64),
            (80, 64),
        ] {
            assert_eq!(SIZES[size_index(needed)], expected, "needed: {needed}");
        }
    }

    #[test]
    fn glyph_tables_match_sizes() {
        for light in [true, false] {
            for (image, size) in glyphs(light).iter().zip(SIZES) {
                assert_eq!(
                    (image.width(), image.height()),
                    (size, size),
                    "light: {light}"
                );
            }
        }
    }
}
