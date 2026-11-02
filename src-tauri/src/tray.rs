//! Tray, screen side and autostart control, without exposing commands to the UI.

use std::sync::mpsc::Sender;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::dock::{Request, Side};
use crate::notes;

pub fn setup(
    app: &mut tauri::App,
    side: Side,
    requests: Sender<Request>,
) -> Result<(), Box<dyn std::error::Error>> {
    // An unreadable registry must not prevent the app from opening.
    let enabled = app.autolaunch().is_enabled().unwrap_or_else(|error| {
        eprintln!("[tray] failed to query autostart: {error}");
        false
    });
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Iniciar com o Windows",
        true,
        enabled,
        None::<&str>,
    )?;
    let side_item = |id, label, value| {
        CheckMenuItem::with_id(app, id, label, true, side == value, None::<&str>)
            .map(|item| (value, item))
    };
    let sides = [
        side_item("side-right", "Lado: direito", Side::Right)?,
        side_item("side-left", "Lado: esquerdo", Side::Left)?,
    ];
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&sides[0].1, &sides[1].1, &separator, &autostart, &quit],
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
            "side-right" => select_side(app, Side::Right, &sides, &requests),
            "side-left" => select_side(app, Side::Left, &sides, &requests),
            "autostart" => {
                let manager = app.autolaunch();
                let result = manager.is_enabled().and_then(|enabled| {
                    if enabled {
                        manager.disable()
                    } else {
                        manager.enable()
                    }
                });
                if let Err(error) = result {
                    eprintln!("[tray] failed to change autostart: {error}");
                }
                match manager.is_enabled() {
                    Ok(enabled) => {
                        if let Err(error) = autostart.set_checked(enabled) {
                            eprintln!("[tray] failed to update check mark: {error}");
                        }
                    }
                    Err(error) => {
                        eprintln!("[tray] failed to query autostart: {error}")
                    }
                }
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Saves the side and moves the panel right away. A failed save still moves it for this session.
fn select_side(
    app: &AppHandle,
    side: Side,
    items: &[(Side, CheckMenuItem<Wry>)],
    requests: &Sender<Request>,
) {
    if let Err(error) = notes::set_side(app, side) {
        eprintln!("[tray] failed to save side: {error}");
    }
    // Clicking toggles the item; the chosen side must stay checked and the other unchecked.
    for (value, item) in items {
        if let Err(error) = item.set_checked(*value == side) {
            eprintln!("[tray] failed to update check mark: {error}");
        }
    }
    // This only fails if the panel thread has stopped.
    let _ = requests.send(Request::SetSide(side));
}
