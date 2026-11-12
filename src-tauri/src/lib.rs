//! App composition: registers modules and commands, without business logic.

mod commands;
mod dock;
mod error;
mod i18n;
mod main_window;
mod notes;
pub mod panel;
mod pinned;
mod platform;
mod shortcut;
mod tray;
mod updates;
mod watch;
mod web;

use std::sync::mpsc;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let commands: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
        commands::app_version,
        updates::exit_ready,
        commands::app_locale,
        commands::app_theme,
        commands::app_set_theme,
        commands::shortcut_activation,
        commands::shortcut_record,
        commands::shortcut_set,
        commands::panel_activate,
        commands::panel_set_pinned,
        commands::panel_close,
        commands::panel_set_side,
        commands::panel_resize,
        commands::web_open,
        commands::web_set_visible,
        commands::web_history,
        commands::web_selection,
        commands::web_suggestions,
        commands::web_addresses,
        commands::web_overlay,
        commands::notes_folder,
        commands::notes_choose,
        commands::notes_list,
        commands::notes_order,
        commands::notes_set_order,
        commands::notes_grouping,
        commands::notes_set_grouping,
        commands::pinned_move,
        commands::notes_open,
        commands::notes_texts,
        commands::notes_save,
        commands::notes_rewrite,
        commands::notes_rename,
        commands::notes_move,
        commands::notes_folders,
        commands::notes_create_folder,
        commands::notes_delete,
        commands::notes_delete_folder,
        commands::notes_undo_delete,
        commands::notes_paste_image,
        commands::notes_leftovers,
        commands::notes_restore,
        commands::pinned_list,
        commands::pinned_add,
        commands::pinned_remove,
        commands::pinned_set_collapsed,
        commands::pinned_restore,
        commands::open_external,
    ];
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            platform::edge::filter(&window, platform::edge::Owner::Ui)
                .map_err(|error| format!("{error:?}"))?;
            let side = notes::side(app.handle());
            let locale = i18n::Locale::current();
            let (requests, receiver) = mpsc::channel();
            tray::setup(app, locale, side, requests.clone())?;
            app.manage(locale);
            updates::start(app.handle());
            shortcut::setup(app, requests.clone());
            app.manage(requests);
            dock::start(
                app.handle().clone(),
                window.hwnd()?.0 as isize,
                receiver,
                side,
                notes::width(app.handle()),
            );
            Ok(())
        })
        // Alt+F4 on the panel would destroy the only UI that can flush saves.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                api.prevent_close();
                updates::request_exit(window.app_handle(), false);
            }
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
        .build(tauri::generate_context!())
        .expect("failed to start application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event
                && !updates::exit_allowed(app)
            {
                api.prevent_exit();
                updates::request_exit(app, false);
            }
        });
}
