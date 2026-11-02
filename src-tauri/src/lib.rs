//! Composição do app: registra módulos e comandos, sem regras de negócio.

mod commands;
mod dock;
mod main_window;
mod notes;
pub mod panel;
mod platform;
mod shortcut;
mod tray;
mod web;

use std::sync::mpsc;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let commands: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
        commands::app_version,
        commands::panel_activate,
        commands::panel_set_pinned,
        commands::panel_set_editing,
        commands::web_open,
        commands::web_close,
        commands::notes_folder,
        commands::notes_choose,
        commands::notes_list,
        commands::notes_open,
    ];
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(notes::IMAGE_SCHEME, commands::note_image)
        .setup(|app| {
            let window = main_window::create(app)?;
            tray::setup(app)?;
            let (requests, receiver) = mpsc::channel();
            shortcut::setup(app, requests.clone());
            app.manage(requests);
            dock::start(window.hwnd()?.0 as isize, receiver);
            Ok(())
        })
        // Comandos do app dispensam capability em origem local: sem esta barreira, a janela da página
        // alcançaria todos eles se chegasse a carregar uma origem do app.
        .invoke_handler(move |invoke| {
            if invoke.message.webview().label() == "main" {
                commands(invoke)
            } else {
                invoke
                    .resolver
                    .reject("comando restrito à janela principal");
                true
            }
        })
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o aplicativo");
}
