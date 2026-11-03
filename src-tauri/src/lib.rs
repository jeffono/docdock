//! App composition: registers modules and commands, without business logic.

mod commands;
mod dock;
mod error;
mod i18n;
mod main_window;
mod markdown;
mod notes;
pub mod panel;
mod platform;
mod shortcut;
mod tray;
mod watch;
mod web;

use std::sync::mpsc;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let commands: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
        commands::app_version,
        commands::app_locale,
        commands::panel_activate,
        commands::panel_set_pinned,
        commands::panel_set_editing,
        commands::web_open,
        commands::web_close,
        commands::notes_folder,
        commands::notes_choose,
        commands::notes_list,
        commands::notes_open,
        commands::notes_save,
        commands::open_external,
    ];
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        // By default a script intercepting `target=_blank` is injected into every window. In the
        // page window, which has no capability, it would cancel the link and fail.
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .register_asynchronous_uri_scheme_protocol(notes::IMAGE_SCHEME, commands::note_image)
        .setup(|app| {
            let window = main_window::create(app)?;
            let side = notes::side(app.handle());
            let locale = i18n::Locale::current();
            let (requests, receiver) = mpsc::channel();
            tray::setup(app, locale, side, requests.clone())?;
            app.manage(locale);
            shortcut::setup(app, requests.clone());
            app.manage(requests);
            dock::start(window.hwnd()?.0 as isize, receiver, side);
            Ok(())
        })
        // App commands need no capability from a local origin. Without this barrier, the page
        // window could reach all of them if it loaded an app origin.
        .invoke_handler(move |invoke| {
            if invoke.message.webview().label() == "main" {
                commands(invoke)
            } else {
                invoke
                    .resolver
                    .reject("command restricted to the main window");
                true
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to start application");
}
