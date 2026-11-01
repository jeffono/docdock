//! Página web em janela própria, sobre o painel. O conteúdo é não confiável: a janela fica fora da
//! capability do app, só navega por http(s) e nunca abre outra janela.

use std::sync::mpsc::Sender;

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

use crate::dock::Request;

const LABEL: &str = "web";

fn is_web(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

fn parse_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|error| format!("Endereço inválido: {error}"))?;
    if is_web(&url) {
        Ok(url)
    } else {
        Err(format!("Só páginas http(s): {}", url.scheme()))
    }
}

/// Links para nova janela, como `target=_blank`, abrem na própria janela da página.
fn navigate_here(app: &AppHandle, url: Url) {
    if is_web(&url)
        && let Some(window) = app.get_webview_window(LABEL)
        && let Err(error) = window.navigate(url)
    {
        eprintln!("[painel] navegar: {error}");
    }
}

/// Navega a janela existente ou cria uma, invisível: quem a posiciona e mostra é a thread do painel.
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
        // Possuída pelo painel: fica sempre acima dele e morre com ele.
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
    fn so_aceita_enderecos_http() {
        assert!(parse_url("https://github.com").is_ok());
        assert!(parse_url("http://example.com").is_ok());
        assert!(parse_url("file:///C:/Windows/win.ini").is_err());
        assert!(parse_url("javascript:alert(1)").is_err());
        assert!(parse_url("github.com").is_err());
    }
}
