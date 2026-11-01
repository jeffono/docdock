//! Composição do app: registra módulos e comandos, sem regras de negócio.

mod commands;
mod dock;
pub mod panel;
mod platform;
mod web;

use std::sync::mpsc;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("janela principal ausente");
            let (requests, receiver) = mpsc::channel();
            app.manage(requests);
            dock::start(window.hwnd()?.0 as isize, receiver);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::panel_activate,
            commands::panel_set_pinned,
            commands::panel_set_editing,
            commands::web_open,
            commands::web_close,
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o aplicativo");
}
