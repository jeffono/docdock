//! Web page in its own window above the panel. Its content is untrusted: the window has no app
//! capability, only navigates over HTTP(S), and never opens another window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

use crate::dock::Request;
use crate::error::Error;
use crate::i18n::Locale;
use crate::platform::browser::{self, Read, Revision};
use crate::platform::edge;

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

/// Iframes bypass `on_navigation`. They keep `about:`, `data:` and `blob:` documents that sites
/// embed, never Tauri's internal origins; external schemes are cancelled separately.
fn frame_allowed(raw: &str) -> bool {
    Url::parse(raw).is_ok_and(|url| !is_internal(&url))
}

fn parse_url(raw: &str) -> Result<Url, Error> {
    let url = Url::parse(raw).map_err(|error| Error::InvalidUrl {
        detail: error.to_string(),
    })?;
    if is_web(&url) {
        Ok(url)
    } else {
        Err(Error::UrlNotAllowed {
            url: url.to_string(),
        })
    }
}

/// Links targeting a new window, such as `target=_blank`, open in the page window itself.
fn navigate_here(app: &AppHandle, url: Url) {
    if is_web(&url)
        && let Some(window) = app.get_webview_window(LABEL)
        && let Err(error) = window.navigate(url)
    {
        eprintln!("[web] navigation: {error}");
    }
}

/// Whether the page window, if any, has its guard, mobile identity and observers. The window is
/// registered before setup, so the lock is held across the whole `open`: a concurrent open must neither
/// navigate that window early nor create a second one. Never taken on the WebView2 UI thread,
/// which `open` waits on.
static READY: Mutex<bool> = Mutex::new(false);
static READING: Mutex<()> = Mutex::new(());

/// Navigates the existing window or creates an invisible one; the panel thread positions and shows it.
pub fn open(app: &AppHandle, requests: &Sender<Request>, raw_url: &str) -> Result<(), Error> {
    let url = parse_url(raw_url)?;
    let mut ready = READY.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(window) = app.get_webview_window(LABEL) {
        // Still registered after a failed setup, until its destruction completes.
        if !*ready {
            return Err(Error::unexpected("page window not guarded"));
        }
        crate::platform::blocker::prepare(&window, crate::blocking::starting(url.as_str()))?;
        window.navigate(url)?;
        let _ = requests.send(Request::SetWebVisible(true));
        return Ok(());
    }
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| Error::unexpected("main window missing"))?;
    let handle = app.clone();
    let build = move || {
        // Owned by the panel: it stays above the panel and is destroyed with it. Blank until the
        // guard is in place: iframes and external schemes bypass `on_navigation`.
        let blank = Url::parse("about:blank").expect("valid fixed URL");
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(blank))
            .theme(crate::notes::theme(app).native())
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
    *ready = false;
    let window = build()?;
    crate::platform::exclude_from_capture(&window);
    let setup = crate::platform::guard(&window, frame_allowed)
        .and_then(|()| edge::filter(&window, edge::Owner::Page, Some(capture_items(app))))
        .and_then(|()| crate::platform::browser::mobile(&window))
        .and_then(|()| crate::platform::browser::observe(&window))
        .and_then(|()| {
            let app = app.clone();
            crate::platform::blocker::install(
                &window,
                crate::blocking::blocked,
                crate::blocking::starting,
                move |url| crate::blocking::committed(&app, url),
            )
        })
        .and_then(|()| {
            let (asking, leaving) = (app.clone(), app.clone());
            crate::platform::permission::install(
                &window,
                move |asked, held| crate::permissions::requested(&asking, asked, held),
                move || crate::permissions::leaving(&leaving),
            )
        });
    if let Err(error) = setup {
        let _ = window.destroy();
        return Err(error);
    }
    *ready = true;
    crate::platform::blocker::prepare(&window, crate::blocking::starting(url.as_str()))?;
    window.navigate(url)?;
    let hwnd = window.hwnd()?;
    let _ = requests.send(Request::AttachWeb(hwnd.0 as isize));
    Ok(())
}

pub fn history(app: &AppHandle, back: bool) -> Result<(), Error> {
    if let Some(window) = app.get_webview_window(LABEL) {
        crate::platform::browser::history(&window, back)?;
    }
    Ok(())
}

/// Long enough for a passage of several pages, short enough to stay a quote in a note.
const SELECTION_MAX_CHARS: usize = 20_000;
/// Chromium's own limit for native titles.
const TITLE_MAX_CHARS: usize = 4_096;
/// The script's slices keep the response under this: at most six JSON bytes per UTF-16 unit.
const SELECTION_MAX_JSON: usize = 256 * 1024;
/// Fixed: no data is ever interpolated. It runs in an isolated world, so these are the browser's
/// own builtins, not the page's. The slices use `TITLE_MAX_CHARS` and `SELECTION_MAX_CHARS`, and
/// `toWellFormed` repairs a surrogate pair they may cut, which strict JSON decoding would reject.
/// The title comes from the same evaluation as the selection: WebView2's native title can still
/// be the previous document's after the next one commits.
const SELECTION_SCRIPT: &str = "[document.title.slice(0, 4096), \
    getSelection().toString().slice(0, 20000)].map((text) => text.toWellFormed())";
/// A hostile or hung page delays the command at most this long.
const SELECTION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(serde::Serialize)]
pub struct PageSelection {
    url: String,
    title: String,
    /// Plain text; empty without a selection or when the page did not answer in time.
    selection: String,
}

#[derive(serde::Deserialize)]
struct Evaluated<T> {
    result: Remote<T>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Remote<T> {
    Object {
        value: T,
    },
    #[serde(other)]
    Other,
}

/// Bounded, typed object value from Runtime.evaluate; all other response types are refused.
pub fn object_value<T: serde::de::DeserializeOwned>(json: &str, max: usize) -> Option<T> {
    if json.len() > max {
        return None;
    }
    match serde_json::from_str::<Evaluated<T>>(json) {
        Ok(Evaluated {
            result: Remote::Object { value },
        }) => Some(value),
        _ => None,
    }
}

/// The two strings of a `Runtime.evaluate` response whose script returns `[string, string]`, if it
/// is at most `max` bytes. The page cannot reach the script's world but owns the text it reads, so
/// anything else is refused.
pub fn two_strings(json: &str, max: usize) -> Option<(String, String)> {
    object_value(json, max)
}

/// Without controls, trimmed and cut at Chromium's limit for native titles.
pub fn clean_title(title: &str) -> String {
    let title: String = title.chars().filter(|c| !c.is_control()).collect();
    title.trim().chars().take(TITLE_MAX_CHARS).collect()
}

/// Decodes a `Runtime.evaluate` response into title and selection. The title keeps no controls;
/// the selection keeps newline and tab, with LF line endings.
fn decode_page(json: &str) -> Option<(String, String)> {
    let (title, text) = two_strings(json, SELECTION_MAX_JSON)?;
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    Some((
        clean_title(&title),
        text.trim().chars().take(SELECTION_MAX_CHARS).collect(),
    ))
}

/// Evaluates `script` in the shown page's main frame. None without a page window or an http(s)
/// page; otherwise its URL and, if the same document answered within `timeout`, the response JSON.
/// Blocks the calling thread, never the WebView2 UI thread.
pub fn read_page(
    app: &AppHandle,
    script: impl Into<String>,
    timeout: Duration,
) -> Result<Option<(String, Option<String>)>, Error> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(None);
    };
    // `Runtime.enable` lists contexts only to the first of overlapping reads, so reads take turns.
    let _turn = READING.lock().unwrap_or_else(PoisonError::into_inner);
    let deadline = Instant::now() + timeout;
    let reply = crate::platform::browser::read(&window, script)?;
    let wait = || reply.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let Ok(Read::Page { url }) = wait() else {
        return Ok(None);
    };
    if !Url::parse(&url).is_ok_and(|url| is_web(&url)) {
        return Ok(None);
    }
    let json = match wait() {
        Ok(Read::Script(json)) => Some(json),
        _ => None,
    };
    Ok(Some((url, json)))
}

/// Current page and its selection in the main frame; selections inside iframes are not read.
/// Blocks the calling thread for at most `SELECTION_TIMEOUT`.
pub fn selection(app: &AppHandle) -> Result<Option<PageSelection>, Error> {
    let Some((url, json)) = read_page(app, SELECTION_SCRIPT, SELECTION_TIMEOUT)? else {
        return Ok(None);
    };
    // Without a script result, no title: the native one may still belong to the previous page.
    let (title, selection) = json.and_then(|json| decode_page(&json)).unwrap_or_default();
    Ok(Some(PageSelection {
        url,
        title,
        selection,
    }))
}

/// Evaluates Rust's `expression` in its isolated world of the page's main frame, taking turns with
/// other reads, and returns the untrusted `Runtime.evaluate` response. None without a page window,
/// while a navigation is in flight or when the page does not answer within `SELECTION_TIMEOUT`.
/// Blocks the calling thread, never the WebView2 UI thread.
pub fn evaluate(app: &AppHandle, expression: String) -> Option<String> {
    read_page(app, expression, SELECTION_TIMEOUT)
        .ok()?
        .and_then(|(_, json)| json)
}

/// Whether the UI would start a capture, as its last report said; the page's context menu reads it
/// as it opens, without waiting on the UI, with the native capture state. The UI checks again when
/// an item is chosen.
static CAPTURE_OFFERED: AtomicBool = AtomicBool::new(false);

pub fn offer_capture(offered: bool) {
    CAPTURE_OFFERED.store(offered, Ordering::Relaxed);
}

/// The page menu's items for the visible area and the full page, in the interface language.
fn capture_items(app: &AppHandle) -> edge::CaptureItems {
    let labels = match *app.state::<Locale>() {
        Locale::En => ["Capture visible area", "Capture full page"],
        Locale::PtBr => ["Capturar área visível", "Capturar página inteira"],
    };
    edge::CaptureItems {
        labels,
        page: || {
            CAPTURE_OFFERED
                .load(Ordering::Relaxed)
                .then(browser::capturable)
                .flatten()
        },
    }
}

/// A large page takes the engine a few seconds to draw and encode; a hung one fails after this.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

/// A screenshot of the shown page and the page it shows: `url` is the address of the document in
/// `image`, `title` only a label (see `browser::Shot`). `clipped`: a full page beyond the limits
/// holds only its start. `shown`: the page window's on-screen epoch as the capture started.
pub struct Capture {
    pub url: String,
    pub title: String,
    pub image: Vec<u8>,
    pub clipped: bool,
    pub shown: u64,
}

/// The visible area, or with `full` the whole page, through `browser::capture`: an http(s) page
/// still at the revision `page` the menu item was chosen on, whose document stays the same, within
/// `CAPTURE_TIMEOUT` of the call, otherwise `CaptureFailed`; a JPEG over the byte limit too,
/// `ImageTooLarge`. Blocks the calling thread, never the WebView2 UI thread.
pub fn capture(app: &AppHandle, full: bool, page: Revision) -> Result<Capture, Error> {
    let shown = crate::permissions::shown();
    let deadline = Instant::now() + CAPTURE_TIMEOUT;
    let window = app.get_webview_window(LABEL).ok_or(Error::CaptureFailed)?;
    let allowed = |url: &str| Url::parse(url).is_ok_and(|url| is_web(&url));
    let reply = browser::capture(&window, full, page, deadline, allowed)?;
    let shot = reply
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| Error::CaptureFailed)??;
    Ok(Capture {
        url: shot.url,
        title: clean_title(&shot.title),
        image: shot.image,
        clipped: shot.clipped,
        shown,
    })
}

/// The panel still shows the page a capture started on: the same revision `page`, and the page
/// window on screen without having left it since `shown`.
pub fn capture_shown(page: Revision, shown: u64) -> bool {
    browser::shows(page) && crate::permissions::still_shown(shown)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchEngine {
    #[default]
    DuckDuckGo,
    Brave,
    Startpage,
    Ecosia,
    Google,
    Bing,
    Custom,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPreferences {
    engine: SearchEngine,
    custom_url: String,
    invalid_custom: bool,
}

impl SearchEngine {
    fn results_template(self) -> &'static str {
        match self {
            Self::DuckDuckGo | Self::Custom => "https://duckduckgo.com/?q=%s",
            Self::Brave => "https://search.brave.com/search?q=%s",
            Self::Startpage => "https://www.startpage.com/sp/search?q=%s",
            Self::Ecosia => "https://www.ecosia.org/search?q=%s",
            Self::Google => "https://www.google.com/search?q=%s",
            Self::Bing => "https://www.bing.com/search?q=%s",
        }
    }

    // Anonymous endpoints shipped in Firefox's search-config-v2.json.
    fn suggestions_endpoint(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::DuckDuckGo => Some(("https://ac.duckduckgo.com/ac/?type=list", "q")),
            Self::Startpage => Some((
                "https://www.startpage.com/suggestions?partner=apex&format=opensearch&segment=startpage.apex.desktop",
                "q",
            )),
            Self::Ecosia => Some(("https://ac.ecosia.org/autocomplete?type=list", "q")),
            Self::Google => Some((
                "https://www.google.com/complete/search?client=firefox&channel=fen",
                "q",
            )),
            Self::Bing => Some(("https://www.bing.com/osjson.aspx?form=OSDJAS", "query")),
            Self::Brave | Self::Custom => None,
        }
    }
}

fn valid_custom_search(template: &str) -> bool {
    let Some((scheme, rest)) = template.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        && template.contains("%s")
        && !authority.is_empty()
        && !authority.contains("%s")
        && !template.contains('\\')
        && !template.chars().any(char::is_control)
        && parse_url(template).is_ok()
}

impl SearchPreferences {
    pub fn new(engine: SearchEngine, custom_url: String) -> Self {
        Self {
            invalid_custom: engine == SearchEngine::Custom && !valid_custom_search(&custom_url),
            engine,
            custom_url,
        }
    }

    pub fn results_url(&self, query: &str) -> Result<String, Error> {
        use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
        const QUERY: &AsciiSet = &NON_ALPHANUMERIC
            .remove(b'-')
            .remove(b'.')
            .remove(b'_')
            .remove(b'~');
        let query = utf8_percent_encode(query, QUERY).to_string();
        if self.engine == SearchEngine::Custom && !self.invalid_custom {
            // URL parsers collapse dot segments even when the dots are percent encoded.
            let traverses_path = self
                .custom_url
                .split(['?', '#'])
                .next()
                .unwrap_or_default()
                .split('/')
                .filter(|segment| segment.contains("%s"))
                .any(|segment| {
                    matches!(
                        segment
                            .replace("%s", &query)
                            .to_ascii_lowercase()
                            .replace("%2e", ".")
                            .as_str(),
                        "." | ".."
                    )
                });
            if traverses_path {
                return Err(Error::SearchPathInvalid);
            }
            return Ok(self.custom_url.replace("%s", &query));
        }
        Ok(self.engine.results_template().replace("%s", &query))
    }
}

const HISTORY_CAP: usize = 300;
static HISTORY: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn visited(mut entries: Vec<String>, raw: &str) -> Vec<String> {
    let Ok(url) = parse_url(raw) else {
        return entries;
    };
    let url = url.to_string();
    if url.len() > 4096 {
        return entries;
    }
    entries.retain(|entry| entry != &url);
    entries.insert(0, url);
    entries.truncate(HISTORY_CAP);
    entries
}

fn decode_history(bytes: &[u8]) -> Vec<String> {
    if bytes.len() > 2 * 1024 * 1024 {
        return vec![];
    }
    let entries: Vec<String> = serde_json::from_slice(bytes).unwrap_or_default();
    // Normalize and deduplicate while keeping most-recent order.
    let mut clean = Vec::new();
    for entry in entries {
        if entry.len() <= 4096
            && let Ok(url) = parse_url(&entry)
        {
            let url = url.to_string();
            if !clean.contains(&url) {
                clean.push(url);
            }
            if clean.len() == HISTORY_CAP {
                break;
            }
        }
    }
    clean
}

pub fn addresses(app: &AppHandle, visit: Option<&str>) -> Vec<String> {
    let _guard = HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(dir) = app.path().app_config_dir() else {
        return vec![];
    };
    let path = dir.join("address-history.json");
    let bytes = crate::blocking::read_bounded(&path, 2 * 1024 * 1024).unwrap_or_default();
    let mut entries = decode_history(&bytes);
    if let Some(url) = visit {
        let updated = visited(entries.clone(), url);
        if updated != entries {
            entries = updated;
            if std::fs::create_dir_all(dir).is_ok()
                && let Ok(bytes) = serde_json::to_vec(&entries)
            {
                let _ = crate::notes::write_atomic(&path, &bytes);
            }
        }
    }
    entries
}

fn phrases(value: serde_json::Value) -> Vec<String> {
    let (items, opensearch) = if value.get(0).is_some_and(|query| query.is_string()) {
        (value.get(1).and_then(|items| items.as_array()), true)
    } else {
        (value.as_array(), false)
    };
    let Some(items) = items else {
        return vec![];
    };
    let mut result = Vec::new();
    for item in items.iter().take(50) {
        let text = if opensearch {
            item.as_str()
        } else {
            item.get("phrase").and_then(|v| v.as_str())
        };
        if let Some(text) = text {
            if text.chars().any(char::is_control) {
                continue;
            }
            let text = text.trim();
            if !text.is_empty() && text.chars().count() <= 200 && !result.iter().any(|s| s == text)
            {
                result.push(text.to_owned());
                if result.len() == 8 {
                    break;
                }
            }
        }
    }
    result
}

/// No cookie store, referer or redirects; bounded body and time, with quiet offline fallback.
pub fn suggestions(app: &AppHandle, query: &str) -> Vec<String> {
    if query.chars().count() < 2 || query.len() > 800 {
        return vec![];
    }
    let Some((endpoint, key)) = crate::notes::search_engine(app).suggestions_endpoint() else {
        return vec![];
    };
    let mut url = Url::parse(endpoint).expect("constant endpoint");
    url.query_pairs_mut().append_pair(key, query);
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    let agent = AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(2)))
            .max_redirects(0)
            .build()
            .into()
    });
    let fetch = || -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut response = agent
            .get(url.as_str())
            .header("Accept", "application/json")
            .header("User-Agent", "DocDock")
            .call()?;
        let bytes = response
            .body_mut()
            .with_config()
            .limit(32 * 1024)
            .read_to_vec()?;
        Ok(phrases(serde_json::from_slice(&bytes)?))
    };
    fetch().unwrap_or_default()
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

    #[test]
    fn custom_search_requires_a_web_template_with_a_fixed_authority() {
        assert!(valid_custom_search("http://example.com/%s"));
        assert!(valid_custom_search("https://example.com/?q=%s"));
        for template in [
            "",
            "https://example.com/",
            "file:///search/%s",
            "https://%s.example.com/search",
            "https://%s@example.com/search",
            "https://ipc.localhost/?q=%s",
        ] {
            assert!(!valid_custom_search(template), "{template}");
        }
    }

    #[test]
    fn invalid_custom_search_uses_default_results() {
        let preferences = SearchPreferences::new(SearchEngine::Custom, "file:///%s".into());
        assert_eq!(
            preferences.results_url("rust").unwrap(),
            "https://duckduckgo.com/?q=rust"
        );
    }

    #[test]
    fn custom_search_substitution_encodes_query_without_injecting_url_components() {
        let preferences = SearchPreferences::new(
            SearchEngine::Custom,
            "https://example.com/search/%s?q=%s".into(),
        );
        let query = "//out.test/?q=olá&x=1#top%";
        let url = parse_url(&preferences.results_url(query).unwrap()).unwrap();
        assert_eq!(url.host_str(), Some("example.com"));
        assert_eq!(
            url.path(),
            "/search/%2F%2Fout.test%2F%3Fq%3Dol%C3%A1%26x%3D1%23top%25"
        );
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![("q".into(), query.into())]
        );
        assert_eq!(url.fragment(), None);
    }

    #[test]
    fn a_query_cannot_traverse_the_custom_search_path() {
        let preferences =
            SearchPreferences::new(SearchEngine::Custom, "https://example.com/search/%s".into());
        for query in [".", ".."] {
            assert_eq!(
                preferences.results_url(query),
                Err(Error::SearchPathInvalid)
            );
        }
    }

    /// A `Runtime.evaluate` response whose selection is the JSON literal `value`.
    fn evaluated(value: &str) -> String {
        format!(r#"{{"result":{{"type":"object","subtype":"array","value":["",{value}]}}}}"#)
    }

    fn decode_selection(json: &str) -> String {
        decode_page(json).map(|(_, text)| text).unwrap_or_default()
    }

    #[test]
    fn frames_keep_embedded_documents_but_never_internal_origins() {
        for url in [
            "https://example.com/embed",
            "about:srcdoc",
            "about:blank",
            "data:text/html,x",
            "blob:https://example.com/1",
        ] {
            assert!(frame_allowed(url), "{url}");
        }
        for url in [
            "http://tauri.localhost/",
            "http://ipc.localhost/",
            "http://note-asset.localhost/x.png",
            "not a url",
        ] {
            assert!(!frame_allowed(url), "{url}");
        }
    }

    #[test]
    fn page_text_must_be_two_strings() {
        for json in [
            r#"{"result":{"type":"number","value":3}}"#,
            r#"{"result":{"type":"string","value":"a"}}"#,
            r#"{"result":{"type":"object","value":{"text":"a"}}}"#,
            r#"{"result":{"type":"object","value":["a"]}}"#,
            r#"{"result":{"type":"object","value":["a","b","c"]}}"#,
            r#"{"result":{"type":"object","value":[1,"b"]}}"#,
            r#"{"result":{"type":"object","subtype":"error"},"exceptionDetails":{"text":"a"}}"#,
            r#"{"result":{"type":"string"}}"#,
            r#"{"result":{"type":"string","value":3}}"#,
            r#"{"result":{"value":"a"}}"#,
            r#""a""#,
            "not json",
        ] {
            assert_eq!(decode_selection(json), "", "{json}");
        }
        assert_eq!(
            decode_selection(&evaluated(r#""quoted text""#)),
            "quoted text"
        );
    }

    #[test]
    fn title_is_bounded_and_without_controls() {
        let title = format!("a\tb\n{}", "é".repeat(TITLE_MAX_CHARS));
        let json = serde_json::json!({ "result": { "type": "object", "value": [title, ""] } });
        let (title, _) = decode_page(&json.to_string()).unwrap();
        assert!(title.starts_with("abé"));
        assert_eq!(title.chars().count(), TITLE_MAX_CHARS);
    }

    #[test]
    fn selection_is_cut_at_the_character_limit() {
        let value = serde_json::to_string(&"é".repeat(SELECTION_MAX_CHARS + 5)).unwrap();
        let text = decode_selection(&evaluated(&value));
        assert_eq!(text.chars().count(), SELECTION_MAX_CHARS);
    }

    #[test]
    fn oversized_selection_json_is_rejected() {
        let value = serde_json::to_string(&"a".repeat(SELECTION_MAX_JSON)).unwrap();
        assert_eq!(decode_selection(&evaluated(&value)), "");
    }

    #[test]
    fn selection_keeps_only_newline_and_tab_controls() {
        assert_eq!(
            decode_selection(&evaluated(r#""a\u0000b\u001bc\u007fd\u0085e\tf\ng""#)),
            "abcde\tf\ng"
        );
    }

    #[test]
    fn selection_line_endings_become_lf() {
        assert_eq!(
            decode_selection(&evaluated(r#""a\r\nb\rc\nd""#)),
            "a\nb\nc\nd"
        );
    }

    #[test]
    fn revisiting_moves_address_to_front_without_duplicates() {
        assert_eq!(
            visited(
                vec!["https://a.com/".into(), "https://b.com/".into()],
                "https://b.com"
            ),
            vec!["https://b.com/", "https://a.com/"]
        );
    }
    #[test]
    fn history_evicts_oldest_at_cap() {
        let entries = (0..HISTORY_CAP)
            .map(|n| format!("https://example.com/{n}"))
            .collect();
        let result = visited(entries, "https://new.com");
        assert_eq!(result.len(), HISTORY_CAP);
        assert!(!result.contains(&format!("https://example.com/{}", HISTORY_CAP - 1)));
    }
    #[test]
    fn corrupt_history_starts_empty() {
        assert!(decode_history(b"broken json").is_empty());
        assert!(decode_history(br#"["https://example.com", 3]"#).is_empty());
    }
    #[test]
    fn suggestion_count_is_bounded() {
        let items: Vec<_> = (0..30)
            .map(|n| serde_json::json!({"phrase": format!("query {n}")}))
            .collect();
        assert_eq!(phrases(serde_json::json!(items)).len(), 8);
    }
    #[test]
    fn history_rejects_non_web_visits() {
        assert!(visited(vec![], "file:///private").is_empty());
    }
    #[test]
    fn untrusted_suggestions_reject_wrong_types_controls_and_long_phrases() {
        assert_eq!(
            phrases(
                serde_json::json!([{}, {"phrase": 3}, {"phrase": "hi\nthere"},
            {"phrase": "x".repeat(201)}, {"phrase": "safe"}, {"phrase": "safe"}])
            ),
            vec!["safe"]
        );
    }

    #[test]
    fn opensearch_suggestions_use_only_valid_unique_strings_from_the_suggestions_list() {
        let payload = serde_json::json!([
            "query",
            [42, {}, {"phrase": "object"}, "\nleading", "has\tcontrol", "x".repeat(201),
                "", " safe ", "safe", "next"],
            ["metadata"]
        ]);
        assert_eq!(phrases(payload), vec!["safe", "next"]);
    }
}
