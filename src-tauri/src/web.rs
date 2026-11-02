//! Web page in its own window above the panel. Its content is untrusted: the window has no app
//! capability, only navigates over HTTP(S), and never opens another window.

use std::sync::mpsc::Sender;

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

use crate::dock::Request;

const LABEL: &str = "web";

/// WebView2 routes any `http://tauri.*`, `ipc.*` or `note-asset.*` to the internal protocol,
/// with or without `.localhost`.
fn is_internal(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default().trim_end_matches('.');
    let label = host.split('.').next().unwrap_or_default();
    host.ends_with(".localhost")
        || (url.scheme() == "http" && matches!(label, "tauri" | "ipc" | "note-asset"))
}

/// Only HTTP(S), outside Tauri's internal origins.
fn is_web(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https") && !is_internal(url)
}

fn parse_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|error| format!("Endereço inválido: {error}"))?;
    if is_web(&url) {
        Ok(url)
    } else {
        Err(format!("Endereço não permitido: {url}"))
    }
}

/// Links targeting a new window, such as `target=_blank`, open in the page window itself.
fn navigate_here(app: &AppHandle, url: Url) {
    if is_web(&url)
        && let Some(window) = app.get_webview_window(LABEL)
        && let Err(error) = window.navigate(url)
    {
        eprintln!("[panel] navigation: {error}");
    }
}

/// Navigates the existing window or creates an invisible one; the panel thread positions and shows it.
pub fn open(app: &AppHandle, requests: &Sender<Request>, raw_url: &str) -> Result<(), String> {
    let url = parse_url(raw_url)?;
    if let Some(window) = app.get_webview_window(LABEL) {
        return window.navigate(url).map_err(|error| error.to_string());
    }
    let main = app
        .get_webview_window("main")
        .ok_or("janela principal ausente")?;
    let handle = app.clone();
    let build = move || {
        // Owned by the panel: it stays above the panel and is destroyed with it.
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(url))
            .owner(&main)?
            .visible(false)
            .focused(false)
            .decorations(false)
            .resizable(false)
            .shadow(false)
            .on_navigation(is_web)
            .on_new_window(move |url, _| {
                navigate_here(&handle, url);
                NewWindowResponse::Deny
            })
            .build()
    };
    let window = build().map_err(|error| error.to_string())?;
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let _ = requests.send(Request::AttachWeb(hwnd.0 as isize));
    Ok(())
}

pub fn close(app: &AppHandle) -> Result<(), String> {
    match app.get_webview_window(LABEL) {
        Some(window) => window.destroy().map_err(|error| error.to_string()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_accepts_http_urls() {
        assert!(parse_url("https://github.com").is_ok());
        assert!(parse_url("http://example.com").is_ok());
        assert!(parse_url("file:///C:/Windows/win.ini").is_err());
        assert!(parse_url("javascript:alert(1)").is_err());
        assert!(parse_url("github.com").is_err());
        assert!(parse_url("http://tauri.localhost/").is_err());
        assert!(parse_url("https://ipc.localhost/").is_err());
        assert!(parse_url("http://tauri.localhost./").is_err());
        assert!(parse_url("http://tauri.app/").is_err());
        assert!(parse_url("http://note-asset.example/").is_err());
        assert!(parse_url("https://tauri.app/").is_ok());
    }
}
