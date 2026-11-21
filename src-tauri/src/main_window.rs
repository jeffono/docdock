//! App windows display untrusted rendered content: they never leave the UI origin or open another
//! window. `main` is created here instead of in `tauri.conf.json` for that reason, and secondary app
//! windows share the same hardening; see `docs/decisions/0014-app-windows.md`.

use tauri::webview::NewWindowResponse;
use tauri::{App, AppHandle, Url, WebviewWindow, WebviewWindowBuilder, Wry};

use crate::error::Error;
use crate::platform::edge::{self, Owner};

/// Tauri's `freezePrototype` applies to every window and would break sites in `web`.
const FREEZE_PROTOTYPE: &str = "Object.freeze(Object.prototype)";

/// In development the UI comes from Vite; in production, from the `tauri` protocol at this Windows URL.
pub fn ui_url(dev_url: Option<&Url>) -> Url {
    dev_url
        .cloned()
        .unwrap_or_else(|| Url::parse("http://tauri.localhost").expect("valid fixed URL"))
}

pub fn stays_in_ui(url: &Url, ui: &Url) -> bool {
    url.origin() == ui.origin()
}

/// Secondary app windows, the image preview and the floating windows Rust registered and has not
/// closed: they reach only their own command list (`by_window` in `lib.rs`) and, with `main`, read
/// note images.
pub fn is_secondary(label: &str) -> bool {
    label == crate::preview::LABEL || crate::floating::is_live(label)
}

/// Windows that may read note images through `note-asset`; any other gets 403.
pub fn reads_images(label: &str) -> bool {
    label == "main" || is_secondary(label)
}

/// Builds an app window with the hardening every one shares: InPrivate, a frozen prototype, no
/// navigation away from the UI, no new windows, no file drops, excluded from capture while still
/// hidden and Edge's keys and menu items filtered.
pub fn app_window(
    app: &AppHandle,
    builder: WebviewWindowBuilder<'_, Wry, AppHandle>,
) -> Result<WebviewWindow, Error> {
    let dev_url = app
        .config()
        .build
        .dev_url
        .clone()
        .filter(|_| tauri::is_dev());
    let ui = ui_url(dev_url.as_ref());
    let window = builder
        .theme(crate::notes::theme(app).native())
        // Notes arrive over IPC and must not become persistent browser site data.
        .incognito(true)
        .initialization_script(FREEZE_PROTOTYPE)
        .on_navigation(move |url| stays_in_ui(url, &ui))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .disable_drag_drop_handler()
        .build()
        .map_err(Error::unexpected)?;
    crate::platform::exclude_from_capture(&window);
    let filtered = edge::filter(&window, Owner::Ui, None);
    destroy_on_error(&window, filtered)?;
    Ok(window)
}

/// A secondary app window: hardened as `main`, and taking no dropped files at all. `main` keeps
/// WebView2's own drops, for images dropped into the editor. Call it off the main thread.
pub fn secondary_window(
    app: &AppHandle,
    builder: WebviewWindowBuilder<'_, Wry, AppHandle>,
) -> Result<WebviewWindow, Error> {
    let window = app_window(app, builder)?;
    let blocked = edge::block_external_drops(&window);
    destroy_on_error(&window, blocked)?;
    Ok(window)
}

/// A window whose setup failed must not stay half-hardened under its label.
fn destroy_on_error(window: &WebviewWindow, result: Result<(), Error>) -> Result<(), Error> {
    if result.is_err() {
        let _ = window.destroy();
    }
    result
}

pub fn create(app: &App) -> Result<WebviewWindow, Error> {
    let builder = WebviewWindowBuilder::from_config(app.handle(), &app.config().app.windows[0])
        .map_err(Error::unexpected)?
        // Beside the panel, a strip as deep as the edge tab shows only the tab the UI draws there.
        .transparent(true);
    app_window(app.handle(), builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stays(url: &str, dev_url: Option<&str>) -> bool {
        let dev_url = dev_url.map(|url| Url::parse(url).unwrap());
        stays_in_ui(&Url::parse(url).unwrap(), &ui_url(dev_url.as_ref()))
    }

    #[test]
    fn only_navigates_within_ui_origin() {
        const DEV: Option<&str> = Some("http://localhost:1420");
        assert!(stays("http://tauri.localhost/", None));
        assert!(stays("http://tauri.localhost/index.html#note", None));
        assert!(stays("http://localhost:1420/src/App.svelte", DEV));
        for url in [
            "https://example.com/",
            "https://tauri.localhost/",
            "http://tauri.localhost:8080/",
            "http://tauri.localhost@example.com/",
            "http://note-asset.localhost/photo.png",
            "http://localhost:1420/",
            "file:///C:/Windows/win.ini",
            "about:blank",
            "data:text/html,hello",
        ] {
            assert!(!stays(url, None), "{url}");
        }
        assert!(!stays("http://tauri.localhost/", DEV));
    }

    // Catches the page or tweak window, or a floating-window label Rust never registered, gaining
    // note images or secondary commands. `float-0` is never given: labels start at 1.
    #[test]
    fn only_app_windows_read_images() {
        assert!(reads_images("main"));
        assert!(reads_images("image-preview"));
        for label in ["web", "tweak", "float-0", "image-preview-2", ""] {
            assert!(!reads_images(label), "{label}");
            assert!(!is_secondary(label), "{label}");
        }
    }
}
