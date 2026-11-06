//! Observe native WebView2 state on its UI thread. No script or message bridge in remote pages.
use tauri::{Emitter, Manager, WebviewWindow};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use webview2_com::{
    CoTaskMemPWSTR, DocumentTitleChangedEventHandler, HistoryChangedEventHandler,
    SourceChangedEventHandler,
};
use windows::core::{BOOL, PWSTR};

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PageState {
    url: String,
    title: String,
    can_go_back: bool,
    can_go_forward: bool,
}

fn report(app: &tauri::AppHandle, page: &ICoreWebView2) -> windows::core::Result<()> {
    unsafe {
        let mut url = PWSTR::null();
        page.Source(&mut url)?;
        let url = CoTaskMemPWSTR::from(url).to_string();
        let mut title = PWSTR::null();
        page.DocumentTitle(&mut title)?;
        let title = CoTaskMemPWSTR::from(title).to_string();
        let (mut back, mut forward) = (BOOL::default(), BOOL::default());
        page.CanGoBack(&mut back)?;
        page.CanGoForward(&mut forward)?;
        let _ = app.emit_to(
            "main",
            "web-state",
            PageState {
                url,
                title,
                can_go_back: back.as_bool(),
                can_go_forward: forward.as_bool(),
            },
        );
    }
    Ok(())
}

pub fn observe(window: &WebviewWindow) -> tauri::Result<()> {
    let app = window.app_handle().clone();
    window.with_webview(move |view| unsafe {
        let install = || -> windows::core::Result<()> {
            let page = view.controller().CoreWebView2()?;
            let handle = app.clone();
            let source = SourceChangedEventHandler::create(Box::new(move |page, _| {
                if let Some(page) = page {
                    let mut source = PWSTR::null();
                    page.Source(&mut source)?;
                    let url = CoTaskMemPWSTR::from(source).to_string();
                    let app = handle.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        crate::web::addresses(&app, Some(&url));
                    });
                    report(&handle, &page)?;
                }
                Ok(())
            }));
            let handle = app.clone();
            let history = HistoryChangedEventHandler::create(Box::new(move |page, _| {
                if let Some(page) = page {
                    report(&handle, &page)?;
                }
                Ok(())
            }));
            let handle = app.clone();
            let title = DocumentTitleChangedEventHandler::create(Box::new(move |page, _| {
                if let Some(page) = page {
                    report(&handle, &page)?;
                }
                Ok(())
            }));
            // WebView2 owns handlers until the webview is destroyed. No captured COM references.
            let mut token = 0;
            page.add_SourceChanged(&source, &mut token)?;
            page.add_HistoryChanged(&history, &mut token)?;
            page.add_DocumentTitleChanged(&title, &mut token)?;
            report(&app, &page)
        };
        if let Err(error) = install() {
            eprintln!("[web] observe: {error}");
        }
    })
}

pub fn history(window: &WebviewWindow, back: bool) -> tauri::Result<()> {
    window.with_webview(move |view| unsafe {
        let navigate = || -> windows::core::Result<()> {
            let page = view.controller().CoreWebView2()?;
            let mut available = BOOL::default();
            if back {
                page.CanGoBack(&mut available)?;
                if available.as_bool() {
                    page.GoBack()?;
                }
            } else {
                page.CanGoForward(&mut available)?;
                if available.as_bool() {
                    page.GoForward()?;
                }
            }
            Ok(())
        };
        if let Err(error) = navigate() {
            eprintln!("[web] history: {error}");
        }
    })
}
