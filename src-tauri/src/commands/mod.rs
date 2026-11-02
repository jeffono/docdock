//! Comandos expostos à UI. Mantenha-os finos: a lógica fica nos módulos de domínio.

use std::sync::mpsc::Sender;

use tauri::{AppHandle, State};

use crate::dock::Request;
use crate::web;

#[tauri::command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn send(requests: State<'_, Sender<Request>>, request: Request) {
    // Só falha se a thread do painel morreu, e aí não há o que fazer pela UI.
    let _ = requests.send(request);
}

#[tauri::command]
pub fn panel_activate(requests: State<'_, Sender<Request>>) {
    send(requests, Request::Activate);
}

#[tauri::command]
pub fn panel_set_pinned(requests: State<'_, Sender<Request>>, pinned: bool) {
    send(requests, Request::SetPinned(pinned));
}

#[tauri::command]
pub fn panel_set_editing(requests: State<'_, Sender<Request>>, editing: bool) {
    send(requests, Request::SetEditing(editing));
}

/// Assíncrono porque criar uma janela em comando síncrono trava no Windows.
#[tauri::command]
pub async fn web_open(
    app: AppHandle,
    requests: State<'_, Sender<Request>>,
    url: String,
) -> Result<(), String> {
    web::open(&app, &requests, &url)
}

#[tauri::command]
pub fn web_close(app: AppHandle) -> Result<(), String> {
    web::close(&app)
}
