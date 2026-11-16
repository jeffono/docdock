//! Development builds only: the tweak window and the commands behind it. `lib.rs` declares this
//! module under `cfg(debug_assertions)`, so release builds contain none of it.
//!
//! The tweak window edits; the panel applies. The window sends its whole state to the panel with the
//! `tweak-state` event, and the panel asks for it with `tweak-request` after a reload. The window
//! reaches no app command: `main_only` rejects them, and its capability only grants events.

use tauri::ipc::CapabilityBuilder;
use tauri::webview::NewWindowResponse;
use tauri::{App, AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const TWEAK: &str = "tweak";

/// Lets the tweak window exchange events with the panel, and nothing else.
pub fn setup(app: &App) -> tauri::Result<()> {
    app.add_capability(
        CapabilityBuilder::new(TWEAK)
            .window(TWEAK)
            .permission("core:event:allow-listen")
            .permission("core:event:allow-unlisten")
            .permission("core:event:allow-emit-to"),
    )
}

/// The panel applies the tweak window's slide motions, from the next slide on.
#[tauri::command]
pub fn dev_motion(motions: crate::motion::Motions) {
    crate::motion::tune(motions);
}

/// Opens the tweak window, an ordinary resizable window, or brings it forward if already open.
/// Asynchronous because creating a window in a synchronous command hangs on Windows.
#[tauri::command]
pub async fn dev_tweak_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(TWEAK) {
        let _ = window.unminimize();
        return window.set_focus().map_err(|error| error.to_string());
    }
    let ui = crate::main_window::ui_url(app.config().build.dev_url.as_ref());
    WebviewWindowBuilder::new(&app, TWEAK, WebviewUrl::App("index.html".into()))
        .title("DocDock tweaks")
        .inner_size(440.0, 860.0)
        .min_inner_size(320.0, 320.0)
        .theme(crate::notes::theme(&app).native())
        .on_navigation(move |url| crate::main_window::stays_in_ui(url, &ui))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
}
