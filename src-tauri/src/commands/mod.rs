//! Commands exposed to the UI. Keep them thin: logic belongs in domain modules.

use std::sync::mpsc::Sender;

use serde::Serialize;
use tauri::{AppHandle, State, UriSchemeContext, UriSchemeResponder, Wry, http};

use crate::dock::Request;
use crate::error::Error;
use crate::i18n::Locale;
use crate::platform::browser::Revision;
use crate::web;

#[tauri::command(async)]
pub fn vault_file(app: AppHandle) -> Result<Option<String>, Error> {
    crate::notes::vault_file(&app)
}
#[tauri::command(async)]
pub fn vault_choose(
    app: AppHandle,
    requests: State<'_, Sender<Request>>,
) -> Result<Option<String>, Error> {
    crate::notes::choose_vault_file(&app, &requests)
}
#[tauri::command(async)]
pub fn vault_clear(app: AppHandle) -> Result<(), Error> {
    crate::notes::clear_vault_file(&app)
}
#[tauri::command(async)]
pub fn vault_unlock(app: AppHandle) -> Result<(), Error> {
    crate::notes::unlock_vault(&app)
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

#[tauri::command]
pub fn app_autostart() -> Result<bool, Error> {
    crate::tray::autostart_enabled()
}

#[tauri::command]
pub fn app_set_autostart(app: AppHandle, enabled: bool) -> Result<(), Error> {
    crate::tray::select_autostart(&app, enabled)
}

/// Synchronous on purpose: registering hotkeys runs on the main thread.
#[tauri::command]
pub fn shortcut_record(app: AppHandle, recording: bool) -> crate::shortcut::Status {
    crate::shortcut::record(&app, recording)
}
#[tauri::command]
pub fn shortcut_set(
    app: AppHandle,
    combination: crate::shortcut::Combination,
) -> Result<crate::shortcut::Status, Error> {
    crate::shortcut::change(&app, combination)
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

/// What the UI needs to draw the edge tab and tell a click on it from a drag.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct TabShape {
    #[serde(flatten)]
    size: crate::handle::Logical,
    /// The system drag tolerance each way, in physical pixels.
    drag: [i32; 2],
}

#[tauri::command]
pub fn panel_tab_shape() -> TabShape {
    let drag = crate::platform::drag_tolerance();
    TabShape {
        size: crate::dock::handle_tuning().tab,
        drag: [drag.x, drag.y],
    }
}

/// The edge tab the UI draws was clicked: pressed and released on it without dragging.
#[tauri::command]
pub fn panel_tab(requests: State<'_, Sender<Request>>) {
    send(requests, Request::Tab);
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

/// A screenshot of the shown page, `full` or its visible area, if it is still at the revision
/// `page` the menu item was chosen on; waits off the async runtime as `web_selection` does. Raw
/// bytes avoid expanding the image into JSON numbers: a little-endian `u32` gives the length of a
/// JSON object with the page's `url`, `title`, whether it was `clipped` to the size limits, the
/// on-screen epoch `shown` for `web_capture_shown`, and the notes `folder` and `folderId` it started
/// in, which `notes_create_with_image` requires; the image follows it.
#[tauri::command]
pub async fn web_capture(
    app: AppHandle,
    full: bool,
    page: Revision,
) -> Result<tauri::ipc::Response, Error> {
    let (capture, (folder, folder_id)) = tauri::async_runtime::spawn_blocking(move || {
        let folder = crate::notes::capture_folder(&app)?;
        web::capture(&app, full, page).map(|capture| (capture, folder))
    })
    .await
    .map_err(Error::unexpected)??;
    let page = serde_json::json!({
        "url": capture.url, "title": capture.title, "clipped": capture.clipped,
        "shown": capture.shown, "folder": folder, "folderId": folder_id,
    })
    .to_string();
    let length = u32::try_from(page.len()).map_err(Error::unexpected)?;
    let mut body = Vec::with_capacity(4 + page.len() + capture.image.len());
    body.extend(length.to_le_bytes());
    body.extend(page.as_bytes());
    body.extend(capture.image);
    Ok(tauri::ipc::Response::new(body))
}

/// Whether the panel still shows the page a capture started on, so its note may open in the editor.
#[tauri::command]
pub fn web_capture_shown(page: Revision, shown: u64) -> bool {
    web::capture_shown(page, shown)
}

#[tauri::command]
pub fn web_offer_capture(offered: bool) {
    web::offer_capture(offered);
}

/// The shown page's article for the reader view; waits off the async runtime as `web_selection`
/// does, and extraction runs there too.
#[tauri::command]
pub async fn web_reader(app: AppHandle) -> Result<Option<crate::reader::Article>, Error> {
    tauri::async_runtime::spawn_blocking(move || crate::reader::read(&app))
        .await
        .map_err(Error::unexpected)?
}

/// Copy and cut in the UI write through here, never through the browser, so the text stays out of
/// clipboard history and cloud clipboard. Off the main thread: opening the clipboard may wait for
/// another app to close it.
#[tauri::command]
pub async fn clipboard_write(window: tauri::WebviewWindow, text: String) -> Result<(), Error> {
    let owner = window.hwnd()?.0 as isize;
    tauri::async_runtime::spawn_blocking(move || crate::platform::write_private_text(owner, &text))
        .await
        .map_err(Error::unexpected)?
        .map_err(Error::unexpected)
}

/// `async`: looking at an offline share can take seconds.
#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn notes_trust(app: AppHandle) -> Result<Option<crate::notes::Folder>, Error> {
    crate::notes::trust(&app)
}
/// The notes, and the subfolders left out because they could not be read, for a discreet notice.
#[tauri::command(async)]
pub fn notes_list(app: AppHandle) -> Result<crate::notes::Listing, Error> {
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
/// As `notes_save`, but a note that is gone answers a conflict instead of being recreated.
#[tauri::command(async)]
pub fn notes_rewrite(
    app: AppHandle,
    path: String,
    content: String,
    base: String,
) -> Result<crate::notes::Saved, Error> {
    crate::notes::rewrite_note(&app, &path, &content, &base)
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
/// Moves a note to `to`, in another folder, as `notes_rename` does.
#[tauri::command(async)]
pub fn notes_move(app: AppHandle, from: String, to: String, base: String) -> Result<(), Error> {
    crate::notes::move_note(&app, &from, &to, &base)
}
#[tauri::command(async)]
pub fn notes_folders(app: AppHandle) -> Result<Vec<crate::notes::FolderEntry>, Error> {
    crate::notes::folders(&app)
}
#[tauri::command(async)]
pub fn notes_create_folder(app: AppHandle, path: String) -> Result<(), Error> {
    crate::notes::create_folder(&app, &path)
}
/// Deletes a folder and everything in it at once, as `notes_delete` does a note.
#[tauri::command(async)]
pub fn notes_delete_folder(app: AppHandle, path: String) -> Result<crate::notes::Removal, Error> {
    crate::notes::delete_folder(&app, &path)
}
/// Moves a note to the Recycle Bin, or to the notes folder's own trash where the Recycle Bin cannot
/// take it, and answers how it can come back.
#[tauri::command(async)]
pub fn notes_delete(app: AppHandle, path: String) -> Result<crate::notes::Removal, Error> {
    crate::notes::delete_note(&app, &path)
}
#[tauri::command(async)]
pub fn notes_undo_delete(app: AppHandle, path: String) -> Result<(), Error> {
    crate::notes::undo_delete(&app, &path)
}
#[tauri::command(async)]
pub fn notes_trash(app: AppHandle) -> Result<Vec<crate::trash::Item>, Error> {
    crate::notes::trash_items(&app)
}
/// Answers the path the item took. The notes list hears of it as of any change.
#[tauri::command(async)]
pub fn notes_trash_restore(app: AppHandle, id: String, undo: bool) -> Result<String, Error> {
    let restored = crate::notes::restore_trashed(&app, &id, undo);
    if !matches!(restored, Err(Error::TrashItemMissing)) {
        crate::watch::notify(&app);
    }
    restored
}
#[tauri::command(async)]
pub fn notes_trash_empty(app: AppHandle, ids: Vec<String>) -> Result<(), Error> {
    crate::notes::empty_trash(&app, &ids)
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
/// A new note holding `content` and then a link to an image put beside it, as
/// `notes_create_with_image` in `notes.rs` says. Raw IPC as `notes_paste_image`: the body is a
/// little-endian `u32` with the length of `content` in UTF-8, `content` and the image. The headers
/// name the note and the notes folder path and marker id `web_capture` started in, so a folder
/// chosen since refuses the note with `folderChanged`.
#[tauri::command(async)]
pub fn notes_create_with_image(
    app: AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<crate::notes::Saved, Error> {
    let header = |name: &str| -> Result<String, Error> {
        let value = request
            .headers()
            .get(name)
            .and_then(|h| h.to_str().ok())
            .ok_or(Error::InvalidPath)?;
        percent_encoding::percent_decode_str(value)
            .decode_utf8()
            .map(|value| value.into_owned())
            .map_err(|_| Error::InvalidPath)
    };
    let (path, folder, folder_id) = (
        header("x-note-path")?,
        header("x-notes-folder")?,
        header("x-notes-folder-id")?,
    );
    let tauri::ipc::InvokeBody::Raw(body) = request.body() else {
        return Err(Error::UnsupportedImage);
    };
    let (length, rest) = body
        .split_first_chunk::<4>()
        .ok_or(Error::UnsupportedImage)?;
    let length = u32::from_le_bytes(*length) as usize;
    let (content, image) = rest
        .split_at_checked(length)
        .ok_or(Error::UnsupportedImage)?;
    let content = std::str::from_utf8(content).map_err(Error::unexpected)?;
    crate::notes::create_with_image(&app, &folder, &folder_id, &path, content, image)
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
pub fn web_search_preferences(app: AppHandle) -> web::SearchPreferences {
    crate::notes::search_preferences(&app)
}

#[tauri::command(async)]
pub fn web_set_search_preferences(
    app: AppHandle,
    engine: web::SearchEngine,
    custom_url: String,
) -> Result<web::SearchPreferences, Error> {
    crate::notes::set_search_preferences(&app, engine, custom_url)
}

#[tauri::command(async)]
pub fn web_search_url(app: AppHandle, query: String) -> Result<String, Error> {
    crate::notes::search_preferences(&app).results_url(&query)
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
pub fn notes_relative_dates(app: AppHandle) -> Result<bool, Error> {
    crate::notes::relative_dates(&app)
}
#[tauri::command]
pub fn notes_set_relative_dates(app: AppHandle, enabled: bool) -> Result<(), Error> {
    crate::notes::set_relative_dates(&app, enabled)
}
#[tauri::command]
pub fn notes_grouping(app: AppHandle) -> Result<crate::notes::Grouping, Error> {
    crate::notes::grouping(&app)
}
#[tauri::command]
pub fn notes_set_grouping(app: AppHandle, grouping: crate::notes::Grouping) -> Result<(), Error> {
    crate::notes::set_grouping(&app, grouping)
}
#[tauri::command(async)]
pub fn pinned_move(
    app: AppHandle,
    target: String,
    to: String,
    base: String,
) -> Result<crate::notes::Pinned, Error> {
    crate::notes::move_pinned(&app, &target, &to, &base)
}

#[tauri::command]
pub fn blocking_status(url: String) -> Option<crate::blocking::SiteStatus> {
    crate::blocking::status(&url)
}
#[tauri::command]
pub fn blocking_sites() -> Vec<String> {
    crate::blocking::allowed_sites()
}
#[tauri::command(async)]
pub fn blocking_set_allowed(app: AppHandle, url: String, allowed: bool) -> Result<(), Error> {
    crate::blocking::set_allowed(&app, &url, allowed)
}

/// Waits on a blocking-pool thread for the WebView2 UI thread, which holds the waiting requests.
#[tauri::command]
pub async fn permission_answer(app: AppHandle, id: u64, allow: bool) -> Result<(), Error> {
    tauri::async_runtime::spawn_blocking(move || crate::permissions::answer(&app, id, allow))
        .await
        .map_err(Error::unexpected)?
}
#[tauri::command]
pub fn permission_list() -> Vec<crate::permissions::Remembered> {
    crate::permissions::remembered()
}
#[tauri::command(async)]
pub fn permission_forget(
    app: AppHandle,
    site: String,
    kind: crate::permissions::Kind,
) -> Result<(), Error> {
    crate::permissions::forget(&app, &site, kind)
}

#[tauri::command]
pub fn note_search_suggestions(app: AppHandle) -> Result<bool, Error> {
    crate::notes::note_search_suggestions(&app)
}
#[tauri::command]
pub fn note_search_set_suggestions(app: AppHandle, enabled: bool) -> Result<(), Error> {
    crate::notes::set_note_search_suggestions(&app, enabled)
}
#[tauri::command]
pub fn spelling_enabled(app: AppHandle) -> Result<bool, Error> {
    crate::notes::spelling(&app)
}
#[tauri::command]
pub fn spelling_set_enabled(app: AppHandle, enabled: bool) -> Result<(), Error> {
    crate::notes::set_spelling(&app, enabled)
}
#[tauri::command(async)]
pub fn spelling_languages(spelling: State<'_, crate::spelling::Spelling>) -> Vec<String> {
    spelling.languages()
}
#[tauri::command(async)]
pub fn spelling_check(
    spelling: State<'_, crate::spelling::Spelling>,
    words: Vec<String>,
) -> Vec<bool> {
    spelling.check(words)
}
#[tauri::command(async)]
pub fn spelling_suggest(
    spelling: State<'_, crate::spelling::Spelling>,
    word: String,
) -> Vec<String> {
    spelling.suggest(word)
}
#[tauri::command(async)]
pub fn spelling_add(
    spelling: State<'_, crate::spelling::Spelling>,
    word: String,
) -> Result<(), Error> {
    if spelling.add(word) {
        Ok(())
    } else {
        Err(Error::unexpected(
            "the word could not be added to the Windows dictionary",
        ))
    }
}
