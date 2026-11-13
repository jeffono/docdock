//! Content blocking in the page window, natively: WebView2 asks before each request and Rust answers
//! from memory on its UI thread. Nothing here gives the page a channel; hiding styles reach it only
//! through Rust's isolated world (`browser::document_start` and `browser::read`).
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_CSP_VIOLATION_REPORT,
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH,
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE,
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_PING,
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET,
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_WEBSOCKET,
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST,
    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL, ICoreWebView2_22,
};
use webview2_com::{
    CoTaskMemPWSTR, ContentLoadingEventHandler, NavigationStartingEventHandler,
    WebResourceRequestedEventHandler,
};
use windows::Win32::System::Com::IStream;
use windows::core::{HSTRING, Interface, PWSTR};

use crate::error::Error;
use crate::platform::browser;

/// A request as the content blocker sees it. `kind` uses the filter-list request types.
pub struct Request<'a> {
    pub url: &'a str,
    /// The page's main-frame URL: third-party rules compare against it.
    pub source: &'a str,
    pub kind: &'static str,
    pub method: &'a str,
}

/// Filter-list request type for a WebView2 resource context. A document is the main frame only
/// when it is the main navigation's target; any other document is a frame.
fn kind(context: COREWEBVIEW2_WEB_RESOURCE_CONTEXT, main: bool) -> &'static str {
    match context {
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT if main => "document",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT => "subdocument",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET => "stylesheet",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE => "image",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA => "media",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT => "font",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT => "script",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST
        | COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH => "xmlhttprequest",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_WEBSOCKET => "websocket",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_PING => "ping",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_CSP_VIOLATION_REPORT => "csp_report",
        _ => "other",
    }
}

/// Asks `blocked` about every request of the window, from every frame and worker, and answers a
/// blocked one with an empty 403 before it reaches the network. `starting` receives the target of
/// each main-frame navigation and returns the script for Rust's isolated world to run when its
/// document starts, if any; `committed` receives the URL of each new main-frame document. Both run
/// on the UI thread, so they must return at once.
/// Only for the page window: the app's UI is never filtered. Waits for the WebView2 UI thread, so
/// never call it from that thread.
pub fn install(
    window: &WebviewWindow,
    blocked: fn(&Request) -> bool,
    starting: fn(&str) -> Option<String>,
    committed: impl Fn(String) + Send + 'static,
) -> Result<(), Error> {
    if window.label() == "main" {
        return Err(Error::unexpected("content blocking is for the page window"));
    }
    let (sender, receiver) = mpsc::channel();
    window.with_webview(move |view| unsafe {
        let install = || -> windows::core::Result<()> {
            let page = view.controller().CoreWebView2()?;
            let environment = view.environment();
            // WebView2 raises NavigationStarting before requesting the main frame's document.
            let main = Rc::new(RefCell::new(String::new()));
            let target = main.clone();
            browser::enable_document_start(&page)?;
            // Also raised for each redirect, so the script follows the final host.
            let starting = NavigationStartingEventHandler::create(Box::new(move |page, args| {
                if let (Some(page), Some(args)) = (page, args) {
                    let mut uri = PWSTR::null();
                    args.Uri(&mut uri)?;
                    let uri = CoTaskMemPWSTR::from(uri).to_string();
                    let script = starting(&uri);
                    *target.borrow_mut() = uri;
                    if let Err(error) = browser::document_start(&page, script, mpsc::channel().0) {
                        eprintln!("[blocking] document start: {error}");
                    }
                }
                Ok(())
            }));
            let requested =
                WebResourceRequestedEventHandler::create(Box::new(move |page, args| {
                    let (Some(page), Some(args)) = (page, args) else {
                        return Ok(());
                    };
                    let request = args.Request()?;
                    let mut uri = PWSTR::null();
                    request.Uri(&mut uri)?;
                    let url = CoTaskMemPWSTR::from(uri).to_string();
                    let mut method = PWSTR::null();
                    request.Method(&mut method)?;
                    let method = CoTaskMemPWSTR::from(method).to_string();
                    let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
                    args.ResourceContext(&mut context)?;
                    let mut source = PWSTR::null();
                    page.Source(&mut source)?;
                    let source = CoTaskMemPWSTR::from(source).to_string();
                    let request = Request {
                        kind: kind(context, *main.borrow() == url),
                        url: &url,
                        source: &source,
                        method: &method,
                    };
                    if blocked(&request) {
                        let response = environment.CreateWebResourceResponse(
                            None::<&IStream>,
                            403,
                            &HSTRING::from("Blocked"),
                            &HSTRING::new(),
                        )?;
                        args.SetResponse(&response)?;
                    }
                    Ok(())
                }));
            let loading = ContentLoadingEventHandler::create(Box::new(move |page, _| {
                if let Some(page) = page {
                    let mut source = PWSTR::null();
                    page.Source(&mut source)?;
                    committed(CoTaskMemPWSTR::from(source).to_string());
                }
                Ok(())
            }));
            // Without the source kinds, requests from workers and some iframes bypass the filter.
            let all = HSTRING::from("*");
            match page.cast::<ICoreWebView2_22>() {
                Ok(page) => page.AddWebResourceRequestedFilterWithRequestSourceKinds(
                    &all,
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                )?,
                Err(_) => {
                    page.AddWebResourceRequestedFilter(&all, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?
                }
            }
            // WebView2 owns handlers until the webview is destroyed.
            let mut token = 0;
            page.add_NavigationStarting(&starting, &mut token)?;
            page.add_WebResourceRequested(&requested, &mut token)?;
            page.add_ContentLoading(&loading, &mut token)
        };
        let _ = sender.send(install());
    })?;
    match receiver.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(Error::unexpected(format!("page blocker: {error}"))),
        Err(_) => Err(Error::unexpected("page blocker not installed")),
    }
}

/// How long a navigation the app starts waits for its document-start script.
const PREPARE_TIMEOUT: Duration = Duration::from_secs(1);

/// Registers `source` as the window's document-start script before a navigation the app starts,
/// such as a shield reload, which `NavigationStarting` alone would register too late for a
/// same-site document. Waits for the WebView2 UI thread, so never call it from that thread.
pub fn prepare(window: &WebviewWindow, source: Option<String>) -> Result<(), Error> {
    let (sender, receiver) = mpsc::channel();
    window.with_webview(move |view| unsafe {
        let register =
            || browser::document_start(&view.controller().CoreWebView2()?, source, sender);
        if let Err(error) = register() {
            eprintln!("[blocking] document start: {error}");
        }
    })?;
    // Without an answer the navigation still happens; the passes hide what they can.
    let _ = receiver.recv_timeout(PREPARE_TIMEOUT);
    Ok(())
}
