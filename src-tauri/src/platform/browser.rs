//! Observe native WebView2 state on its UI thread. No message bridge in remote pages: Rust may run a
//! fixed script in an isolated world and read its result, but the page has nothing it can call.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, PoisonError};

use serde_json::{Value, json};
use tauri::{Emitter, Manager, WebviewWindow};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BALANCED, ICoreWebView2, ICoreWebView2_13,
    ICoreWebView2Profile3, ICoreWebView2Settings2,
};
use webview2_com::{
    CallDevToolsProtocolMethodCompletedHandler, CoTaskMemPWSTR, ContentLoadingEventHandler,
    DevToolsProtocolEventReceivedEventHandler, DocumentTitleChangedEventHandler,
    HistoryChangedEventHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    SourceChangedEventHandler,
};
use windows::core::{BOOL, HSTRING, Interface, PWSTR};

use crate::error::Error;

/// Keep the installed engine's versions; only change its desktop identity.
fn mobile_user_agent(default: &str) -> Option<String> {
    let start = default.find("(Windows ")?;
    let end = start + default[start..].find(')')? + 1;
    let mut mobile = default.to_owned();
    mobile.replace_range(start..end, "(Linux; Android 10; K)");
    Some(
        mobile
            .replace(" Safari/", " Mobile Safari/")
            .replace(" Edg/", " EdgA/"),
    )
}

/// Prepare only the page window, while still blank. Settings alone leaves Windows Client Hints
/// on WebView2 156; CDP supplies matching metadata for headers and navigator.userAgentData.
/// The override belongs to the WebView target and survives document/renderer changes.
/// Wait off the UI thread for CDP's acknowledgement before allowing the first navigation.
pub fn mobile(window: &WebviewWindow) -> Result<(), Error> {
    let (sender, receiver) = mpsc::channel();
    window.with_webview(move |view| unsafe {
        let prepare = || -> windows::core::Result<()> {
            let page = view.controller().CoreWebView2()?;
            // Only the page profile, before navigation; older runtimes may lack these interfaces.
            // Balanced, not strict: strict is profile-wide and blocks the large ad networks even on
            // sites the shield allows, leaving no way out when blocking breaks a site.
            let tracking = || -> windows::core::Result<()> {
                page.cast::<ICoreWebView2_13>()?
                    .Profile()?
                    .cast::<ICoreWebView2Profile3>()?
                    .SetPreferredTrackingPreventionLevel(
                        COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BALANCED,
                    )
            };
            if let Err(error) = tracking() {
                eprintln!("[web] balanced tracking prevention: {error}");
            }
            let settings = page.Settings()?.cast::<ICoreWebView2Settings2>()?;
            let mut default = PWSTR::null();
            settings.UserAgent(&mut default)?;
            let mobile = mobile_user_agent(&CoTaskMemPWSTR::from(default).to_string())
                .ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
            settings.SetUserAgent(&HSTRING::from(&mobile))?;
            // Omitted brand/version lists use the runtime's own values. No engine version or
            // hardware capabilities are invented, and touch emulation stays unchanged.
            let params = json!({
                "userAgent": mobile,
                "userAgentMetadata": {
                    "platform": "Android", "platformVersion": "10.0.0",
                    "architecture": "", "model": "K", "mobile": true,
                    "bitness": "", "wow64": false, "formFactors": ["Mobile"],
                },
            });
            let reply = sender.clone();
            let done =
                CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _| {
                    let _ = reply.send(result);
                    Ok(())
                }));
            page.CallDevToolsProtocolMethod(
                &HSTRING::from("Emulation.setUserAgentOverride"),
                &HSTRING::from(params.to_string()),
                &done,
            )
        };
        if let Err(error) = prepare() {
            let _ = sender.send(Err(error));
        }
    })?;
    match receiver.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(Error::unexpected(format!("page mobile identity: {error}"))),
        Err(_) => Err(Error::unexpected("page mobile identity not installed")),
    }
}

/// Main-frame navigations of the page window, so a result can be tied to one document even across
/// a same-URL reload or A → B → A. Only the WebView2 UI thread changes it, and there is a single
/// page window.
#[derive(Default)]
struct Navigations {
    /// Changes when a navigation starts and when a document commits.
    changes: u64,
    /// Started and neither committed nor finished.
    pending: Vec<u64>,
}

impl Navigations {
    fn starting(&mut self, id: u64) {
        self.changes += 1;
        self.pending.push(id);
    }

    /// `ContentLoading`: a new document replaced the old one. Older navigations can no longer
    /// commit, so they stop counting even if WebView2 never reports them finished.
    fn committed(&mut self, id: u64) {
        self.changes += 1;
        self.pending.retain(|pending| *pending > id);
    }

    /// `NavigationCompleted`: also raised for navigations that failed or were cancelled.
    fn finished(&mut self, id: u64) {
        self.pending.retain(|pending| *pending != id);
    }

    /// None while a navigation is in flight: its document may commit in the middle of a read,
    /// before WebView2 tells the UI thread.
    fn identity(&self) -> Option<u64> {
        self.pending.is_empty().then_some(self.changes)
    }
}

static NAVIGATIONS: Mutex<Navigations> = Mutex::new(Navigations {
    changes: 0,
    pending: Vec::new(),
});

fn navigations() -> std::sync::MutexGuard<'static, Navigations> {
    NAVIGATIONS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Changes when a main-frame navigation of the page window starts or commits, including a reload.
/// Same-document navigations, such as a route change, keep it.
pub fn navigation() -> u64 {
    navigations().changes
}

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

/// Installs the observers before the page loads anything: reads trust the navigation state only
/// because these handlers see every navigation. Waits for the WebView2 UI thread, so never call it
/// from that thread.
pub fn observe(window: &WebviewWindow) -> Result<(), Error> {
    let app = window.app_handle().clone();
    let (sender, receiver) = mpsc::channel();
    window.with_webview(move |view| unsafe {
        let install = || -> windows::core::Result<()> {
            let page = view.controller().CoreWebView2()?;
            // A previous page window, destroyed while loading, leaves nothing in flight.
            navigations().pending.clear();
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
            let starting = NavigationStartingEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    let mut id = 0;
                    args.NavigationId(&mut id)?;
                    navigations().starting(id);
                }
                Ok(())
            }));
            let committed = ContentLoadingEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    let mut id = 0;
                    args.NavigationId(&mut id)?;
                    navigations().committed(id);
                }
                Ok(())
            }));
            let finished = NavigationCompletedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    let mut id = 0;
                    args.NavigationId(&mut id)?;
                    navigations().finished(id);
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
            page.add_NavigationStarting(&starting, &mut token)?;
            page.add_ContentLoading(&committed, &mut token)?;
            page.add_NavigationCompleted(&finished, &mut token)?;
            report(&app, &page)
        };
        let _ = sender.send(install());
    })?;
    match receiver.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(Error::unexpected(format!("page observers: {error}"))),
        Err(_) => Err(Error::unexpected("page observers not installed")),
    }
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

/// What `read` reports, in order: the native URL, then the script result. A failed step drops
/// the sender instead, so the receiver sees a disconnect.
pub enum Read {
    /// The page's URL; its title comes only from the script, in the same document.
    Page { url: String },
    /// `Runtime.evaluate` response JSON, sent only if it came from the document of `Page`.
    Script(String),
}

/// Isolated world for Rust's own scripts: it shares the DOM but not the page's globals or
/// prototypes, and the page cannot reach it.
const WORLD: &str = "docdock";

/// Reads the URL from WebView2, then evaluates Rust's own expression in an isolated world of the
/// main frame through the DevTools protocol. The page cannot call Rust: only these one-shot
/// completion handlers receive results, which are untrusted. The page cannot override the
/// expression's builtins there, so the expression's own length limit bounds the response.
/// Nothing is evaluated while a navigation is in flight, and the result is dropped unless the
/// document is still the one whose URL was reported.
/// The handlers run on the WebView2 UI thread, so the caller waits on the receiver from another thread.
pub fn read(
    window: &WebviewWindow,
    expression: impl Into<String>,
) -> tauri::Result<Receiver<Read>> {
    let expression = expression.into();
    let (sender, receiver) = mpsc::channel();
    window.with_webview(move |view| unsafe {
        let run = || -> windows::core::Result<()> {
            let page = view.controller().CoreWebView2()?;
            let url = source(&page)?;
            let identity = navigations().identity();
            let _ = sender.send(Read::Page { url: url.clone() });
            match identity {
                Some(identity) => evaluate(&page, Document { identity, url }, expression, sender),
                None => Ok(()),
            }
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
    expression: String,
    reply: Sender<Read>,
) -> windows::core::Result<()> {
    call(
        page,
        "Target.getTargetInfo",
        json!({}),
        move |page, info| {
            let Some(frame) = parse(&info)["targetInfo"]["targetId"]
                .as_str()
                .map(str::to_owned)
            else {
                return Ok(());
            };
            let world = json!({ "frameId": frame, "worldName": WORLD });
            call(
                page,
                "Page.createIsolatedWorld",
                world,
                move |page, world| match parse(&world)["executionContextId"].as_i64() {
                    Some(id) => unique_context(page, frame, id, move |page, context| {
                        evaluate_in(page, context, document, expression, reply)
                    }),
                    None => Ok(()),
                },
            )
        },
    )
}

/// The world's `uniqueId`. Chromium may reuse a numeric context id for another context after a
/// cross-process navigation, even the page's own world, where its prototypes rule again and the
/// response is unbounded; a `uniqueId` names one context of one document only. Only
/// `Runtime.enable` reports it, so the domain stays enabled just long enough to list the contexts.
/// `then` runs only if exactly one context matches.
fn unique_context(
    page: &ICoreWebView2,
    frame: String,
    id: i64,
    then: impl FnOnce(&ICoreWebView2, String) -> windows::core::Result<()> + 'static,
) -> windows::core::Result<()> {
    let found = Rc::new(RefCell::new(Vec::new()));
    let sink = found.clone();
    let created = DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
        if let Some(args) = args {
            let mut event = PWSTR::null();
            unsafe { args.ParameterObjectAsJson(&mut event)? };
            let event = parse(&CoTaskMemPWSTR::from(event).to_string());
            if let Some(context) = world_context(&event, &frame, id) {
                sink.borrow_mut().push(context);
            }
        }
        Ok(())
    }));
    let events = unsafe {
        page.GetDevToolsProtocolEventReceiver(&HSTRING::from("Runtime.executionContextCreated"))?
    };
    let mut token = 0;
    unsafe { events.add_DevToolsProtocolEventReceived(&created, &mut token)? };
    let target = page.clone();
    let receiver = events.clone();
    // Success or failure, neither the handler nor the domain outlives this read.
    let done = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _| {
        let _ = unsafe { receiver.remove_DevToolsProtocolEventReceived(token) };
        let _ = call(&target, "Runtime.disable", json!({}), |_, _| Ok(()));
        match (result.is_ok(), found.take().as_slice()) {
            (true, [context]) => then(&target, context.clone()),
            _ => Ok(()),
        }
    }));
    // Enabling reports every existing context before its response.
    let enabled = unsafe {
        page.CallDevToolsProtocolMethod(
            &HSTRING::from("Runtime.enable"),
            &HSTRING::from("{}"),
            &done,
        )
    };
    if enabled.is_err() {
        let _ = unsafe { events.remove_DevToolsProtocolEventReceived(token) };
    }
    enabled
}

/// The `uniqueId` of an `executionContextCreated` event for this read's world: isolated, named
/// `WORLD`, in the main frame, with the id `Page.createIsolatedWorld` returned. Without a
/// `uniqueId` the browser would not know `uniqueContextId` either, and might ignore it and
/// evaluate in the page's own world, so the read fails closed.
fn world_context(event: &Value, frame: &str, id: i64) -> Option<String> {
    let context = &event["context"];
    let aux = &context["auxData"];
    let ours = context["id"].as_i64() == Some(id)
        && context["name"] == WORLD
        && aux["type"] == "isolated"
        && aux["isDefault"] == false
        && aux["frameId"] == frame;
    let unique = context["uniqueId"]
        .as_str()
        .filter(|unique| !unique.is_empty());
    unique.filter(|_| ours).map(str::to_owned)
}

/// Evaluates in the context named by `context` only, and only if the document is unchanged before
/// and after.
fn evaluate_in(
    page: &ICoreWebView2,
    context: String,
    document: Document,
    expression: String,
    reply: Sender<Read>,
) -> windows::core::Result<()> {
    if !unchanged(page, &document) {
        return Ok(());
    }
    let script = json!({
        "expression": expression,
        "uniqueContextId": context,
        "returnByValue": true,
        "silent": true,
    });
    call(page, "Runtime.evaluate", script, move |page, result| {
        if unchanged(page, &document) {
            let _ = reply.send(Read::Script(result));
        }
        Ok(())
    })
}

/// The script Rust's isolated world runs as main-frame documents start. One browser call at a time,
/// so every answer applies to what the state knows. Only the WebView2 UI thread uses it, and there
/// is a single page window.
#[derive(Default)]
struct DocumentStart {
    /// What the browser confirmed: the source and its identifier, kept until its removal is too.
    registered: Option<(String, String)>,
    /// The source navigations should get.
    wanted: Option<String>,
    /// A browser call is in flight; its answer advances the state.
    busy: bool,
    /// Sent to once `registered` matches `wanted`, or a call fails.
    waiters: Vec<Sender<()>>,
}

/// A browser call that brings the registered script closer to the wanted one.
#[derive(Debug, PartialEq)]
enum Change {
    Add(String),
    Remove(String),
}

impl DocumentStart {
    fn want(&mut self, source: Option<String>, settled: Sender<()>) {
        self.wanted = source;
        self.waiters.push(settled);
    }

    /// The call to make next, if any is due; settles the waiters once nothing is.
    fn next(&mut self) -> Option<Change> {
        if self.busy {
            return None;
        }
        let change = match (&self.registered, &self.wanted) {
            (Some((source, _)), Some(wanted)) if source == wanted => None,
            (Some((_, identifier)), _) => Some(Change::Remove(identifier.clone())),
            (None, Some(wanted)) => Some(Change::Add(wanted.clone())),
            (None, None) => None,
        };
        match change {
            Some(_) => self.busy = true,
            None => self.settle(),
        }
        change
    }

    /// Records the browser's answer to `change`: the new script's identifier, or whether the
    /// removal succeeded. A failure keeps what is registered and lets waiters go; the next
    /// navigation tries again. Returns whether to continue.
    fn answered(&mut self, change: Change, identifier: Option<String>, ok: bool) -> bool {
        self.busy = false;
        match change {
            Change::Add(source) if ok => match identifier {
                Some(identifier) => self.registered = Some((source, identifier)),
                None => return self.failed(),
            },
            Change::Remove(_) if ok => self.registered = None,
            _ => return self.failed(),
        }
        true
    }

    /// A call failed or could not be made.
    fn failed(&mut self) -> bool {
        self.busy = false;
        self.settle();
        false
    }

    fn settle(&mut self) {
        for waiter in self.waiters.drain(..) {
            let _ = waiter.send(());
        }
    }
}

thread_local! {
    static DOCUMENT_START: RefCell<DocumentStart> = RefCell::default();
}

/// Lets document-start scripts run for this page: Chromium evaluates them only while the `Page`
/// domain is enabled. The domain only reports events to this DevTools session; the page cannot
/// notice it. Forgets a previous page window's script.
pub fn enable_document_start(page: &ICoreWebView2) -> windows::core::Result<()> {
    DOCUMENT_START.take();
    call(page, "Page.enable", json!({}), |_, _| Ok(()))
}

/// Replaces the document-start script with `source` in the `WORLD` isolated world, or removes it,
/// and sends on `settled` once the browser has confirmed it, or a call failed (at once when
/// `source` is already registered). A script runs in every frame of the documents created after
/// the browser registers it, so the script itself skips the frames and hosts it is not for.
/// Registered as a same-site navigation starts, it may arrive after the new document: an unchanged
/// script is therefore kept, and the app registers before the navigations it starts itself.
pub fn document_start(
    page: &ICoreWebView2,
    source: Option<String>,
    settled: Sender<()>,
) -> windows::core::Result<()> {
    DOCUMENT_START.with_borrow_mut(|state| state.want(source, settled));
    advance(page)
}

/// Makes the next call the document-start state asks for; its answer makes the one after.
fn advance(page: &ICoreWebView2) -> windows::core::Result<()> {
    let Some(change) = DOCUMENT_START.with_borrow_mut(DocumentStart::next) else {
        return Ok(());
    };
    let (method, params) = match &change {
        Change::Add(source) => (
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": source, "worldName": WORLD }),
        ),
        Change::Remove(identifier) => (
            "Page.removeScriptToEvaluateOnNewDocument",
            json!({ "identifier": identifier }),
        ),
    };
    let target = page.clone();
    let done = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
        let answer = parse(&json);
        // A protocol error may also arrive as its JSON.
        let ok = result.is_ok() && answer.get("code").is_none();
        let identifier = answer["identifier"].as_str().map(str::to_owned);
        if !DOCUMENT_START.with_borrow_mut(|state| state.answered(change, identifier, ok)) {
            eprintln!("[blocking] document start: {json}");
            return Ok(());
        }
        advance(&target)
    }));
    let sent = unsafe {
        page.CallDevToolsProtocolMethod(
            &HSTRING::from(method),
            &HSTRING::from(params.to_string()),
            &done,
        )
    };
    if sent.is_err() {
        DOCUMENT_START.with_borrow_mut(DocumentStart::failed);
    }
    sent
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

/// The document a read started on: its settled navigation identity and URL.
struct Document {
    identity: u64,
    url: String,
}

/// No navigation started or committed since `document`, none is in flight, and the URL is the same.
fn unchanged(page: &ICoreWebView2, document: &Document) -> bool {
    navigations().identity() == Some(document.identity)
        && unsafe { source(page) }.is_ok_and(|url| url == document.url)
}

unsafe fn source(page: &ICoreWebView2) -> windows::core::Result<String> {
    let mut url = PWSTR::null();
    unsafe { page.Source(&mut url)? };
    Ok(CoTaskMemPWSTR::from(url).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_identity_preserves_the_installed_engine_versions() {
        assert_eq!(
            mobile_user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.1234.5 Safari/537.36 Edg/156.0.2345.6").as_deref(),
            Some("Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.1234.5 Mobile Safari/537.36 EdgA/156.0.2345.6")
        );
    }

    #[test]
    fn a_navigation_in_flight_has_no_identity() {
        let mut navigations = Navigations::default();
        navigations.starting(1);
        assert_eq!(navigations.identity(), None);
    }

    #[test]
    fn a_commit_gives_the_new_document_another_identity() {
        let mut navigations = Navigations::default();
        let before = navigations.identity();
        navigations.starting(1);
        navigations.committed(1);
        assert!(navigations.identity().is_some());
        assert_ne!(navigations.identity(), before);
    }

    #[test]
    fn a_cancelled_navigation_settles_when_finished() {
        let mut navigations = Navigations::default();
        navigations.starting(1);
        navigations.finished(1);
        assert!(navigations.identity().is_some());
    }

    #[test]
    fn finishing_one_navigation_leaves_another_in_flight() {
        let mut navigations = Navigations::default();
        navigations.starting(1);
        navigations.starting(2);
        navigations.finished(2);
        assert_eq!(navigations.identity(), None);
    }

    #[test]
    fn a_commit_settles_older_navigations() {
        let mut navigations = Navigations::default();
        navigations.starting(1);
        navigations.starting(2);
        navigations.committed(2);
        assert!(navigations.identity().is_some());
    }

    fn created(context: Value) -> Value {
        json!({ "context": context })
    }

    fn world(id: i64, frame: &str) -> Value {
        json!({
            "id": id,
            "name": WORLD,
            "uniqueId": "-1.2",
            "auxData": { "isDefault": false, "type": "isolated", "frameId": frame },
        })
    }

    #[test]
    fn only_this_reads_world_is_evaluated() {
        assert_eq!(
            world_context(&created(world(3, "F")), "F", 3).as_deref(),
            Some("-1.2")
        );
        let mut main = world(3, "F");
        main["name"] = json!("");
        main["auxData"] = json!({ "isDefault": true, "type": "default", "frameId": "F" });
        for (context, frame, id) in [
            (main, "F", 3),
            (world(3, "iframe"), "F", 3),
            (world(4, "F"), "F", 3),
        ] {
            assert_eq!(world_context(&created(context), frame, id), None);
        }
    }

    #[test]
    fn a_context_without_unique_id_is_never_evaluated() {
        let mut context = world(3, "F");
        context.as_object_mut().unwrap().remove("uniqueId");
        assert_eq!(world_context(&created(context), "F", 3), None);
    }

    fn want(state: &mut DocumentStart, source: Option<&str>) -> Receiver<()> {
        let (sender, receiver) = mpsc::channel();
        state.want(source.map(str::to_owned), sender);
        receiver
    }

    fn registered(source: &str) -> DocumentStart {
        let mut state = DocumentStart::default();
        drop(want(&mut state, Some(source)));
        state.next();
        state.answered(Change::Add(source.into()), Some("1".into()), true);
        state.next();
        state
    }

    #[test]
    fn an_equal_request_during_a_registration_waits_for_its_answer() {
        let mut state = DocumentStart::default();
        let first = want(&mut state, Some("a"));
        assert_eq!(state.next(), Some(Change::Add("a".into())));
        let second = want(&mut state, Some("a"));
        assert_eq!(state.next(), None);
        assert!(second.try_recv().is_err());
        state.answered(Change::Add("a".into()), Some("1".into()), true);
        assert_eq!(state.next(), None);
        assert!(first.try_recv().is_ok() && second.try_recv().is_ok());
    }

    #[test]
    fn a_request_during_a_removal_waits_for_its_answer() {
        let mut state = registered("a");
        let first = want(&mut state, None);
        assert_eq!(state.next(), Some(Change::Remove("1".into())));
        let second = want(&mut state, None);
        assert_eq!(state.next(), None);
        assert!(second.try_recv().is_err());
        state.answered(Change::Remove("1".into()), None, true);
        assert_eq!(state.next(), None);
        assert!(first.try_recv().is_ok() && second.try_recv().is_ok());
    }

    #[test]
    fn a_failed_removal_keeps_the_identifier_for_the_next_request() {
        let mut state = registered("a");
        let waiter = want(&mut state, None);
        state.next();
        assert!(!state.answered(Change::Remove("1".into()), None, false));
        assert!(waiter.try_recv().is_ok());
        drop(want(&mut state, None));
        assert_eq!(state.next(), Some(Change::Remove("1".into())));
    }

    #[test]
    fn a_call_that_could_not_be_made_is_retried_by_an_equal_request() {
        let mut state = DocumentStart::default();
        let waiter = want(&mut state, Some("a"));
        state.next();
        state.failed();
        assert!(waiter.try_recv().is_ok());
        drop(want(&mut state, Some("a")));
        assert_eq!(state.next(), Some(Change::Add("a".into())));
    }

    #[test]
    fn a_script_replaced_while_registering_is_removed_before_the_next() {
        let mut state = DocumentStart::default();
        drop(want(&mut state, Some("a")));
        state.next();
        drop(want(&mut state, Some("b")));
        state.answered(Change::Add("a".into()), Some("1".into()), true);
        assert_eq!(state.next(), Some(Change::Remove("1".into())));
        state.answered(Change::Remove("1".into()), None, true);
        assert_eq!(state.next(), Some(Change::Add("b".into())));
    }
}
