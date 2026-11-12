//! The main window displays untrusted rendered content: it never leaves the UI origin or opens
//! another window. It is created here instead of in `tauri.conf.json` for that reason.

use tauri::webview::NewWindowResponse;
use tauri::{App, Url, WebviewWindow, WebviewWindowBuilder};

/// Tauri's `freezePrototype` applies to every window and would break sites in `web`.
const FREEZE_PROTOTYPE: &str = "Object.freeze(Object.prototype)";

/// In development the UI comes from Vite; in production, from the `tauri` protocol at this Windows URL.
fn ui_url(dev_url: Option<&Url>) -> Url {
    dev_url
        .cloned()
        .unwrap_or_else(|| Url::parse("http://tauri.localhost").expect("valid fixed URL"))
}

fn stays_in_ui(url: &Url, ui: &Url) -> bool {
    url.origin() == ui.origin()
}

pub fn create(app: &App) -> tauri::Result<WebviewWindow> {
    let config = app.config();
    let dev_url = config.build.dev_url.as_ref().filter(|_| tauri::is_dev());
    let ui = ui_url(dev_url);
    WebviewWindowBuilder::from_config(app.handle(), &config.app.windows[0])?
        .theme(crate::notes::theme(app.handle()).native())
        // Notes arrive over IPC and must not become persistent browser site data.
        .incognito(true)
        .initialization_script(FREEZE_PROTOTYPE)
        .on_navigation(move |url| stays_in_ui(url, &ui))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .build()
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
        assert!(stays("http://tauri.localhost/index.html#nota", None));
        assert!(stays("http://localhost:1420/src/App.svelte", DEV));
        for url in [
            "https://example.com/",
            "https://tauri.localhost/",
            "http://tauri.localhost:8080/",
            "http://tauri.localhost@example.com/",
            "http://note-asset.localhost/foto.png",
            "http://localhost:1420/",
            "file:///C:/Windows/win.ini",
            "about:blank",
            "data:text/html,oi",
        ] {
            assert!(!stays(url, None), "{url}");
        }
        assert!(!stays("http://tauri.localhost/", DEV));
    }
}
