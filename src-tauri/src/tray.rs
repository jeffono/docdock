//! Tray: its icon, and the screen side and autostart controls it shares with Preferences.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use tauri::image::Image;
use tauri::include_image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::dock::{Request, Side};
use crate::i18n::Locale;
use crate::notes;
use crate::platform::{autostart, taskbar};

struct SideItems([(Side, CheckMenuItem<Wry>); 2]);
struct AutostartItem(CheckMenuItem<Wry>);

/// `window` is a top-level window whose messages tell when the taskbar's theme or scale changes.
pub fn setup(
    app: &mut tauri::App,
    window: isize,
    locale: Locale,
    side: Side,
    requests: Sender<Request>,
) -> Result<(), Box<dyn std::error::Error>> {
    let [right, left, start_with_windows, quit] = match locale {
        Locale::En => ["Side: right", "Side: left", "Start with Windows", "Quit"],
        Locale::PtBr => [
            "Lado: direito",
            "Lado: esquerdo",
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
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", quit, true, None::<&str>)?;
    let update = crate::updates::menu_item(app, locale)?;
    let menu = Menu::with_items(
        app,
        &[
            &sides[0].1,
            &sides[1].1,
            &separator,
            &autostart,
            &update,
            &quit,
        ],
    )?;
    let shown = glyph();
    let tray = TrayIconBuilder::new()
        .icon(glyph_image(shown))
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
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
        .build(app)?;
    // Without this, the glyph keeps the theme and scale it started with.
    let (changes, received) = mpsc::channel();
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
/// Not every scale change on the taskbar's monitor reaches a top-level window (an auto-hidden
/// taskbar keeps the work area), so the glyph is also checked this often. Reading it is cheap.
const RECHECK: Duration = Duration::from_secs(3);

/// Replaces the glyph when the taskbar's theme or scale changed. Messages come in bursts, mostly
/// about something else, and the shell may be the one broadcasting them: the glyph is read once they
/// settle, off the window's thread, and sent to the shell only if it differs.
fn follow_taskbar(tray: &TrayIcon, mut shown: Glyph, changes: &Receiver<()>) {
    loop {
        match changes.recv_timeout(RECHECK) {
            Ok(()) => while changes.recv_timeout(SETTLE).is_ok() {},
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
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

    fn size_for(needed: u32) -> u32 {
        SIZES[size_index(needed)]
    }

    #[test]
    fn common_scales_get_their_exact_size() {
        // 100%, 125%, 150%, 200% and 300%.
        for needed in [16, 20, 24, 32, 48] {
            assert_eq!(size_for(needed), needed);
        }
    }

    #[test]
    fn scales_between_sizes_get_the_next_larger() {
        // 175%, 225% and 350%.
        assert_eq!(size_for(28), 30);
        assert_eq!(size_for(36), 40);
        assert_eq!(size_for(56), 64);
    }

    #[test]
    fn scales_beyond_the_largest_get_the_largest() {
        assert_eq!(size_for(80), 64);
    }

    #[test]
    fn an_unreadable_size_gets_the_smallest() {
        assert_eq!(size_for(0), 16);
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
