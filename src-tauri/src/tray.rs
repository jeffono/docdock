//! Tray and autostart control, without exposing commands to the UI.

use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri_plugin_autostart::ManagerExt;

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
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
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&autostart, &quit])?;
    TrayIconBuilder::new()
        .icon(
            app.default_window_icon()
                .expect("default icon missing")
                .clone(),
        )
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
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
