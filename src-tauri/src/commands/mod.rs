//! Commands exposed to the UI. Keep them thin: logic belongs in domain modules.

use std::sync::mpsc::Sender;

use tauri::{AppHandle, State, UriSchemeContext, UriSchemeResponder, Wry, http};

use crate::dock::Request;
use crate::error::Error;
use crate::i18n::Locale;
use crate::web;

#[tauri::command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub fn app_locale(locale: State<'_, Locale>) -> Locale {
    *locale
}

fn send(requests: State<'_, Sender<Request>>, request: Request) {
    // This only fails if the panel thread has stopped, in which case the UI cannot help.
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

/// Asynchronous because creating a window in a synchronous command hangs on Windows.
#[tauri::command]
pub async fn web_open(
    app: AppHandle,
    requests: State<'_, Sender<Request>>,
    url: String,
) -> Result<(), Error> {
    web::open(&app, &requests, &url)
}

#[tauri::command]
pub fn web_close(app: AppHandle) -> Result<(), Error> {
    web::close(&app)
}

#[tauri::command]
pub fn notes_folder(app: AppHandle) -> Result<Option<crate::notes::Folder>, Error> {
    crate::notes::folder(&app)
}
#[tauri::command]
pub async fn notes_choose(
    app: AppHandle,
    requests: State<'_, Sender<Request>>,
) -> Result<Option<crate::notes::Folder>, Error> {
    crate::notes::choose(&app, &requests)
}
#[tauri::command]
pub fn notes_list(app: AppHandle) -> Result<Vec<crate::notes::Note>, Error> {
    crate::notes::list_for(&app)
}
/// `async` moves reading and sanitization off the main thread.
#[tauri::command(async)]
pub fn notes_open(app: AppHandle, path: String) -> Result<crate::notes::OpenNote, Error> {
    crate::notes::open(&app, &path)
}
/// `async` keeps file I/O off the main thread.
#[tauri::command(async)]
pub fn notes_save(
    app: AppHandle,
    path: String,
    content: String,
    base: String,
) -> Result<crate::notes::Saved, Error> {
    crate::notes::save_note(&app, &path, &content, &base)
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), Error> {
    use tauri_plugin_opener::OpenerExt;
    let parsed = tauri::Url::parse(&url).map_err(|e| Error::InvalidUrl {
        detail: e.to_string(),
    })?;
    if !matches!(parsed.scheme(), "http" | "https" | "mailto") {
        return Err(Error::LinkNotAllowed);
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(Error::unexpected)
}

/// Outside the command barrier in `lib.rs`: the protocol exists in every window and Tauri identifies
/// the requester. Runs on another thread so file reads do not block the WebView.
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
