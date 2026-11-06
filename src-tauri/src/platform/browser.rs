//! Observe native WebView2 state on its UI thread. No message bridge in remote pages: Rust may run a
//! fixed script in an isolated world and read its result, but the page has nothing it can call.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use serde_json::{Value, json};
use tauri::{Emitter, Manager, WebviewWindow};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use webview2_com::{
    CallDevToolsProtocolMethodCompletedHandler, CoTaskMemPWSTR, DocumentTitleChangedEventHandler,
    HistoryChangedEventHandler, NavigationStartingEventHandler, SourceChangedEventHandler,
};
use windows::core::{BOOL, HSTRING, PWSTR};

/// Main-frame navigations started in the page window, so a result can be tied to one document even
/// across a same-URL reload or A → B → A. One counter suffices: there is a single page window.
static NAVIGATIONS: AtomicU64 = AtomicU64::new(0);

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
            let navigation = NavigationStartingEventHandler::create(Box::new(|_, _| {
                NAVIGATIONS.fetch_add(1, Ordering::Relaxed);
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
            page.add_NavigationStarting(&navigation, &mut token)?;
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

/// What `read` reports, in order: the native state, then the script result. A failed step drops
/// the sender instead, so the receiver sees a disconnect.
pub enum Read {
    Page {
        url: String,
        title: String,
    },
    /// `Runtime.evaluate` response JSON; `None` if a navigation started or the URL changed meanwhile.
    Script(Option<String>),
}

/// Isolated world for Rust's own scripts: it shares the DOM but not the page's globals or
/// prototypes, and the page cannot reach it.
const WORLD: &str = "docdock";

/// Reads URL and title from WebView2, then evaluates a fixed expression in an isolated world of the
/// main frame through the DevTools protocol. The page cannot call Rust: only these one-shot
/// completion handlers receive results, which are untrusted. The page cannot override the
/// expression's builtins there, so the expression's own length limit bounds the response.
/// The handlers run on the WebView2 UI thread, so the caller waits on the receiver from another thread.
pub fn read(window: &WebviewWindow, expression: &'static str) -> tauri::Result<Receiver<Read>> {
    let (sender, receiver) = mpsc::channel();
    window.with_webview(move |view| unsafe {
        let run = || -> windows::core::Result<()> {
            let page = view.controller().CoreWebView2()?;
            let document = current(&page)?;
            let mut title = PWSTR::null();
            page.DocumentTitle(&mut title)?;
            let title = CoTaskMemPWSTR::from(title).to_string();
            let _ = sender.send(Read::Page {
                url: document.1.clone(),
                title,
            });
            evaluate(&page, document, expression, sender)
        };
        if let Err(error) = run() {
            eprintln!("[web] read: {error}");
        }
    })?;
    Ok(receiver)
}

/// The page target's id is its main frame's id; the world is reused per document by name.
fn evaluate(
    page: &ICoreWebView2,
    document: Document,
    expression: &'static str,
    reply: Sender<Read>,
) -> windows::core::Result<()> {
    call(
        page,
        "Target.getTargetInfo",
        json!({}),
        move |page, info| {
            let info = parse(&info);
            let Some(frame) = info["targetInfo"]["targetId"].as_str() else {
                return Ok(());
            };
            let world = json!({ "frameId": frame, "worldName": WORLD });
            call(
                page,
                "Page.createIsolatedWorld",
                world,
                move |page, world| match parse(&world)["executionContextId"].as_i64() {
                    Some(context) => evaluate_in(page, context, document, expression, reply),
                    None => Ok(()),
                },
            )
        },
    )
}

/// The result counts only if no navigation started and the URL is unchanged since `document`.
fn evaluate_in(
    page: &ICoreWebView2,
    context: i64,
    document: Document,
    expression: &'static str,
    reply: Sender<Read>,
) -> windows::core::Result<()> {
    let script = json!({
        "expression": expression,
        "contextId": context,
        "returnByValue": true,
        "silent": true,
    });
    call(page, "Runtime.evaluate", script, move |page, result| {
        let same = unsafe { current(page) }.is_ok_and(|now| now == document);
        let _ = reply.send(Read::Script(same.then_some(result)));
        Ok(())
    })
}

/// Calls a DevTools protocol method; `then` receives the response JSON only on success.
fn call(
    page: &ICoreWebView2,
    method: &str,
    params: Value,
    then: impl FnOnce(&ICoreWebView2, String) -> windows::core::Result<()> + 'static,
) -> windows::core::Result<()> {
    let target = page.clone();
    let done = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
        if result.is_ok() {
            then(&target, json)?;
        }
        Ok(())
    }));
    let params = HSTRING::from(params.to_string());
    unsafe { page.CallDevToolsProtocolMethod(&HSTRING::from(method), &params, &done) }
}

fn parse(json: &str) -> Value {
    serde_json::from_str(json).unwrap_or_default()
}

/// Identifies the shown document: navigations started so far and the URL.
type Document = (u64, String);

unsafe fn current(page: &ICoreWebView2) -> windows::core::Result<Document> {
    let navigations = NAVIGATIONS.load(Ordering::Relaxed);
    Ok((navigations, unsafe { source(page)? }))
}

unsafe fn source(page: &ICoreWebView2) -> windows::core::Result<String> {
    let mut url = PWSTR::null();
    unsafe { page.Source(&mut url)? };
    Ok(CoTaskMemPWSTR::from(url).to_string())
}
