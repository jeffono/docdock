//! Shared tray, screen side and autostart control.

use std::sync::mpsc::Sender;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::dock::{Request, Side};
use crate::i18n::Locale;
use crate::notes;
use crate::platform::autostart;

struct SideItems([(Side, CheckMenuItem<Wry>); 2]);
struct AutostartItem(CheckMenuItem<Wry>);

pub fn setup(
    app: &mut tauri::App,
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
    TrayIconBuilder::new()
        .icon(
            app.default_window_icon()
                .expect("default icon missing")
                .clone(),
        )
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
                let result = autostart::is_enabled()
                    .map_err(crate::error::Error::unexpected)
                    .and_then(|enabled| select_autostart(app, !enabled));
                if let Err(error) = result {
                    eprintln!("[tray] failed to change autostart: {error:?}");
                }
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Reads the registry, updates the tray and reports the same state to an open Preferences view.
pub fn autostart_enabled(app: &AppHandle) -> Result<bool, crate::error::Error> {
    let enabled = autostart::is_enabled().map_err(crate::error::Error::unexpected)?;
    app.state::<AutostartItem>()
        .0
        .set_checked(enabled)
        .map_err(crate::error::Error::unexpected)?;
    app.emit_to("main", "autostart-changed", enabled)
        .map_err(crate::error::Error::unexpected)?;
    Ok(enabled)
}

pub fn select_autostart(app: &AppHandle, enabled: bool) -> Result<(), crate::error::Error> {
    let saved = autostart::set_enabled(enabled).map_err(crate::error::Error::unexpected);
    // Even a partial failure (for example clearing StartupApproved) must show the actual state.
    let refreshed = autostart_enabled(app);
    saved.and(refreshed.map(|_| ()))
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
