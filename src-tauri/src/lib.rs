//! App composition: registers modules and commands, without business logic.

mod attachments;
mod blocking;
mod commands;
#[cfg(debug_assertions)]
mod dev;
mod dock;
mod error;
mod floating;
mod floats;
mod handle;
mod i18n;
mod main_window;
mod motion;
mod notes;
mod panel;
mod paste_source;
mod permissions;
mod pinned;
mod platform;
mod preview;
mod reader;
mod screenshot;
mod shortcut;
mod spelling;
mod trash;
mod tray;
mod updates;
mod watch;
mod web;

use std::sync::mpsc;

use tauri::Manager;

/// App commands need no capability from a local origin. Without this barrier, the page window
/// could reach all of them if it loaded an app origin. `main` reaches `main`'s list; a secondary app
/// window (`is_secondary`) reaches only `secondary`, a disjoint list whose commands also check their
/// caller's label; every other window reaches nothing. See `docs/decisions/0014-app-windows.md`.
pub fn by_window<R: tauri::Runtime>(
    main: impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,
    secondary: impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,
    is_secondary: impl Fn(&str) -> bool + Send + Sync + 'static,
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    move |invoke| {
        let webview = invoke.message.webview();
        let label = webview.label();
        if label == "main" {
            main(invoke)
        } else if is_secondary(label) {
            secondary(invoke)
        } else {
            invoke.resolver.reject("command restricted to app windows");
            true
        }
    }
}

/// The production rule for secondary app windows, for `by_window` and its tests.
pub fn is_secondary_window(label: &str) -> bool {
    main_window::is_secondary(label)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let commands: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
        commands::vault_file,
        commands::vault_choose,
        commands::vault_clear,
        commands::vault_unlock,
        updates::exit_ready,
        updates::update_status,
        updates::update_check,
        updates::update_restart,
        commands::app_locale,
        commands::app_theme,
        commands::app_set_theme,
        commands::app_autostart,
        commands::app_set_autostart,
        commands::shortcut_record,
        commands::shortcut_set,
        commands::panel_activate,
        commands::panel_set_pinned,
        commands::panel_close,
        commands::panel_set_side,
        commands::panel_resize,
        commands::panel_tab_shape,
        commands::panel_tab,
        #[cfg(debug_assertions)]
        dev::dev_motion,
        #[cfg(debug_assertions)]
        dev::dev_handle,
        #[cfg(debug_assertions)]
        dev::dev_tab_shape,
        #[cfg(debug_assertions)]
        dev::dev_fonts,
        #[cfg(debug_assertions)]
        dev::dev_tweak_window,
        commands::blocking_status,
        commands::blocking_sites,
        commands::blocking_set_allowed,
        commands::permission_answer,
        commands::permission_list,
        commands::permission_forget,
        commands::web_search_preferences,
        commands::web_set_search_preferences,
        commands::web_search_url,
        commands::web_open,
        commands::web_set_visible,
        commands::web_history,
        commands::web_selection,
        commands::web_reader,
        commands::web_capture,
        commands::web_capture_shown,
        commands::web_offer_capture,
        commands::web_suggestions,
        commands::web_addresses,
        commands::web_overlay,
        commands::notes_folder,
        commands::notes_choose,
        commands::notes_trust,
        commands::notes_list,
        commands::notes_order,
        commands::notes_set_order,
        commands::notes_relative_dates,
        commands::notes_set_relative_dates,
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
        commands::notes_send_pending,
        commands::notes_trash,
        commands::notes_trash_restore,
        commands::notes_trash_empty,
        commands::notes_unused_images,
        commands::notes_recycle_images,
        commands::notes_paste_image,
        commands::notes_create_with_image,
        commands::notes_paste_target,
        commands::notes_leftovers,
        commands::notes_restore,
        commands::pinned_list,
        commands::pinned_add,
        commands::pinned_remove,
        commands::pinned_set_collapsed,
        commands::pinned_restore,
        commands::open_external,
        commands::clipboard_write,
        commands::note_search_suggestions,
        commands::note_search_set_suggestions,
        commands::spelling_enabled,
        commands::spelling_set_enabled,
        commands::spelling_languages,
        commands::spelling_check,
        commands::spelling_suggest,
        commands::spelling_add,
        commands::image_preview_show,
        commands::image_preview_hide,
        commands::floating_open,
        commands::floating_open_note,
        commands::floating_edit_current,
        commands::floating_list,
        commands::floating_close_path,
    ];
    let secondary: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
        commands::image_preview_state,
        commands::float_state,
        commands::float_set_opacity,
        commands::float_set_click_through,
        commands::float_close,
        commands::float_zoom,
        commands::float_natural,
        commands::float_note,
        commands::float_set_text_scale,
        commands::float_edit,
        commands::float_drag,
        commands::float_resize,
    ];
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
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
            #[cfg(debug_assertions)]
            dev::setup(app)?;
            let window = main_window::create(app).map_err(|error| format!("{error:?}"))?;
            let side = notes::side(app.handle());
            let locale = i18n::Locale::current();
            let (requests, receiver) = mpsc::channel();
            let hwnd = window.hwnd()?.0 as isize;
            tray::setup(app, hwnd, locale, side, requests.clone())?;
            app.manage(locale);
            app.manage(spelling::Spelling::start());
            notes::purge_trash(app.handle());
            updates::start(app.handle(), hwnd);
            blocking::start(app.handle());
            permissions::start(app.handle());
            watch::monitor(app.handle().clone());
            shortcut::setup(app, requests.clone());
            app.manage(requests);
            dock::start(
                app.handle().clone(),
                hwnd,
                receiver,
                side,
                notes::width(app.handle()),
            );
            floating::start(app.handle());
            floating::folder_returned(app.handle());
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
            floating::window_event(window.label(), event);
        })
        .invoke_handler(by_window(commands, secondary, is_secondary_window))
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
