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

#[tauri::command]
pub fn app_theme(app: AppHandle) -> crate::notes::Theme {
    crate::notes::theme(&app)
}
#[tauri::command]
pub fn app_set_theme(app: AppHandle, theme: crate::notes::Theme) -> Result<(), Error> {
    crate::notes::set_theme(&app, theme)
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
pub fn panel_close(requests: State<'_, Sender<Request>>) {
    send(requests, Request::Close);
}

#[tauri::command]
pub fn panel_set_side(
    app: AppHandle,
    requests: State<'_, Sender<Request>>,
    side: crate::dock::Side,
) -> Result<(), Error> {
    crate::tray::select_side(&app, side, &requests)
}

#[tauri::command]
pub fn panel_resize(requests: State<'_, Sender<Request>>, width: Option<u32>, done: bool) {
    send(requests, Request::Resize { width, done });
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
pub fn web_set_visible(requests: State<'_, Sender<Request>>, visible: bool) {
    send(requests, Request::SetWebVisible(visible));
}

#[tauri::command]
pub fn web_history(app: AppHandle, back: bool) -> Result<(), Error> {
    web::history(&app, back)
}

/// Waits on a blocking-pool thread while the WebView2 UI thread runs the selection script, so a
/// hung page holds neither the main thread nor an async runtime worker that other commands need.
#[tauri::command]
pub async fn web_selection(app: AppHandle) -> Result<Option<web::PageSelection>, Error> {
    tauri::async_runtime::spawn_blocking(move || web::selection(&app))
        .await
        .map_err(Error::unexpected)?
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
/// Many notes in one call for the search index; unreadable ones are left out.
#[tauri::command(async)]
pub fn notes_texts(
    app: AppHandle,
    paths: Vec<String>,
) -> Result<Vec<crate::notes::NoteText>, Error> {
    crate::notes::texts(&app, &paths)
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
/// `title` is the note's old and new title, for the link text of its pinned items.
#[tauri::command(async)]
pub fn notes_rename(
    app: AppHandle,
    from: String,
    to: String,
    base: String,
    title: (String, String),
) -> Result<(), Error> {
    crate::notes::rename_note(&app, &from, &to, &base, (&title.0, &title.1))
}
/// Moves a note to the Recycle Bin; `notes_undo_delete` brings it back while the panel stays open.
#[tauri::command(async)]
pub fn notes_delete(app: AppHandle, path: String) -> Result<(), Error> {
    crate::notes::delete_note(&app, &path)
}
#[tauri::command(async)]
pub fn notes_undo_delete(app: AppHandle, path: String) -> Result<(), Error> {
    crate::notes::undo_delete(&app, &path)
}
/// Raw IPC avoids expanding image bytes into JSON numbers.
#[tauri::command(async)]
pub fn notes_paste_image(
    app: AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<String, Error> {
    let path = request
        .headers()
        .get("x-note-path")
        .and_then(|h| h.to_str().ok())
        .ok_or(Error::InvalidPath)?;
    let path = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .map_err(|_| Error::InvalidPath)?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(Error::UnsupportedImage);
    };
    crate::notes::paste_image(&app, &path, bytes)
}
#[tauri::command(async)]
pub fn notes_leftovers(app: AppHandle) -> Result<Vec<crate::notes::Leftover>, Error> {
    crate::notes::leftovers(&app)
}
#[tauri::command(async)]
pub fn notes_restore(app: AppHandle, path: String) -> Result<String, Error> {
    crate::notes::restore(&app, &path)
}
/// The pinned commands return the state after the change, so the UI needs no second call.
#[tauri::command(async)]
pub fn pinned_list(app: AppHandle) -> Result<crate::notes::Pinned, Error> {
    crate::notes::pinned(&app)
}
#[tauri::command(async)]
pub fn pinned_add(
    app: AppHandle,
    target: String,
    title: String,
) -> Result<crate::notes::Pinned, Error> {
    crate::notes::pin(&app, &target, &title)
}
#[tauri::command(async)]
pub fn pinned_remove(app: AppHandle, target: String) -> Result<crate::notes::Pinned, Error> {
    crate::notes::unpin(&app, &target)
}
#[tauri::command(async)]
pub fn pinned_set_collapsed(
    app: AppHandle,
    target: String,
    collapsed: bool,
) -> Result<crate::notes::Pinned, Error> {
    crate::notes::set_collapsed(&app, &target, collapsed)
}
#[tauri::command(async)]
pub fn pinned_restore(app: AppHandle) -> Result<crate::notes::Pinned, Error> {
    crate::notes::restore_pinned(&app)
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

#[tauri::command(async)]
pub fn web_suggestions(app: AppHandle, query: String) -> Vec<String> {
    web::suggestions(&app, &query)
}
#[tauri::command(async)]
pub fn web_addresses(app: AppHandle) -> Vec<String> {
    web::addresses(&app, None)
}
#[tauri::command]
pub fn web_overlay(requests: State<'_, Sender<Request>>, height: u32) {
    send(requests, Request::SetWebOverlay(height.min(16384)));
}

#[tauri::command]
pub fn notes_order(app: AppHandle) -> Result<crate::notes::NoteOrder, Error> {
    crate::notes::note_order(&app)
}
#[tauri::command]
pub fn notes_set_order(app: AppHandle, order: crate::notes::NoteOrder) -> Result<(), Error> {
    crate::notes::set_note_order(&app, order)
}
#[tauri::command]
pub fn notes_grouping(app: AppHandle) -> Result<crate::notes::Grouping, Error> {
    crate::notes::grouping(&app)
}
#[tauri::command]
pub fn notes_set_grouping(app: AppHandle, grouping: crate::notes::Grouping) -> Result<(), Error> {
    crate::notes::set_grouping(&app, grouping)
}
#[tauri::command]
pub fn pinned_move(
    app: AppHandle,
    target: String,
    to: String,
    base: String,
) -> Result<crate::notes::Pinned, Error> {
    crate::notes::move_pinned(&app, &target, &to, &base)
}
