//! Comandos expostos à UI. Mantenha-os finos: a lógica fica nos módulos de domínio.

use std::sync::mpsc::Sender;

use tauri::{AppHandle, State, UriSchemeContext, UriSchemeResponder, Wry, http};

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

#[tauri::command]
pub fn notes_folder(app: AppHandle) -> Result<Option<crate::notes::Folder>, String> {
    crate::notes::folder(&app)
}
#[tauri::command]
pub async fn notes_choose(
    app: AppHandle,
    requests: State<'_, Sender<Request>>,
) -> Result<Option<crate::notes::Folder>, String> {
    crate::notes::choose(&app, &requests)
}
#[tauri::command]
pub fn notes_list(app: AppHandle) -> Result<Vec<crate::notes::Note>, String> {
    crate::notes::list_for(&app)
}
/// `async` tira a leitura e a sanitização da thread principal.
#[tauri::command(async)]
pub fn notes_open(app: AppHandle, path: String) -> Result<String, String> {
    crate::notes::open(&app, &path).map(|text| crate::markdown::render(&text, &path))
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let parsed = tauri::Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https" | "mailto") {
        return Err("Esquema inválido".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Fora da barreira de comandos de `lib.rs`: o protocolo existe em todas as janelas e o Tauri informa
/// qual delas pediu. Roda em outra thread para a leitura do arquivo não travar o WebView.
pub fn note_image(
    context: UriSchemeContext<'_, Wry>,
    request: http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = context.app_handle().clone();
    let webview = context.webview_label().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        responder.respond(crate::notes::image_for(
            &app,
            &webview,
            request.uri().path(),
        ));
    });
}
