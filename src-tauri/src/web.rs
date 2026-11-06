//! Web page in its own window above the panel. Its content is untrusted: the window has no app
//! capability, only navigates over HTTP(S), and never opens another window.

use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

use crate::dock::Request;
use crate::error::Error;
use crate::platform::browser::Read;

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
        eprintln!("[panel] navigation: {error}");
    }
}

/// Navigates the existing window or creates an invisible one; the panel thread positions and shows it.
pub fn open(app: &AppHandle, requests: &Sender<Request>, raw_url: &str) -> Result<(), Error> {
    let url = parse_url(raw_url)?;
    if let Some(window) = app.get_webview_window(LABEL) {
        window.navigate(url)?;
        let _ = requests.send(Request::SetWebVisible(true));
        return Ok(());
    }
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| Error::unexpected("main window missing"))?;
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
    let window = build()?;
    crate::platform::browser::observe(&window)?;
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
/// The script's slice keeps the response under this: at most six JSON bytes per UTF-16 unit.
const SELECTION_MAX_JSON: usize = 256 * 1024;
/// Fixed: no data is ever interpolated. It runs in an isolated world, so these are the browser's
/// own builtins, not the page's. `slice` uses `SELECTION_MAX_CHARS` and `toWellFormed` repairs a
/// surrogate pair it may cut, which strict JSON decoding would reject.
const SELECTION_SCRIPT: &str = "getSelection().toString().slice(0, 20000).toWellFormed()";
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
struct Evaluated {
    result: Remote,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Remote {
    String {
        value: String,
    },
    #[serde(other)]
    Other,
}

/// Decodes a `Runtime.evaluate` response. The page cannot reach the script's world but owns the
/// selected text, so only a string result is accepted, without controls other than newline and
/// tab, with LF line endings.
fn decode_selection(json: &str) -> String {
    if json.len() > SELECTION_MAX_JSON {
        return String::new();
    }
    let Ok(Evaluated {
        result: Remote::String { value: text },
    }) = serde_json::from_str(json)
    else {
        return String::new();
    };
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    text.trim().chars().take(SELECTION_MAX_CHARS).collect()
}

/// Current page and its selection in the main frame; selections inside iframes are not read.
/// Blocks the calling thread, never the WebView2 UI thread, for at most `SELECTION_TIMEOUT`.
pub fn selection(app: &AppHandle) -> Result<Option<PageSelection>, Error> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(None);
    };
    let deadline = Instant::now() + SELECTION_TIMEOUT;
    let reply = crate::platform::browser::read(&window, SELECTION_SCRIPT)?;
    let wait = || reply.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let Ok(Read::Page { url, title }) = wait() else {
        return Ok(None);
    };
    if !Url::parse(&url).is_ok_and(|url| is_web(&url)) {
        return Ok(None);
    }
    let selection = match wait() {
        Ok(Read::Script(Some(json))) => decode_selection(&json),
        _ => String::new(),
    };
    Ok(Some(PageSelection {
        url,
        title,
        selection,
    }))
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

    /// A `Runtime.evaluate` response whose string result is the JSON literal `value`.
    fn evaluated(value: &str) -> String {
        format!(r#"{{"result":{{"type":"string","value":{value}}}}}"#)
    }

    #[test]
    fn selection_must_be_a_string_result() {
        for json in [
            r#"{"result":{"type":"number","value":3}}"#,
            r#"{"result":{"type":"object","value":{"text":"a"}}}"#,
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
}

/// Only the supported engine is persisted; unknown values use the privacy default.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchEngine {
    #[default]
    #[serde(other)]
    DuckDuckGo,
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
    use std::io::Read;
    let bytes = std::fs::File::open(&path)
        .and_then(|file| {
            let mut bytes = Vec::new();
            file.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .unwrap_or_default();
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
    let Some(items) = value.as_array() else {
        return vec![];
    };
    let mut result = Vec::new();
    for item in items.iter().take(50) {
        if let Some(text) = item.get("phrase").and_then(|v| v.as_str()) {
            let text = text.trim();
            if !text.is_empty()
                && text.chars().count() <= 200
                && !text.chars().any(char::is_control)
                && !result.iter().any(|s| s == text)
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
    let endpoint = match crate::notes::search_engine(app) {
        SearchEngine::DuckDuckGo => "https://duckduckgo.com/ac/",
    };
    let mut url = Url::parse(endpoint).expect("constant endpoint");
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("type", "json");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(2)))
        .max_redirects(0)
        .build()
        .into();
    let fetch = || -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut response = agent.get(url.as_str()).call()?;
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
mod address_tests {
    use super::*;
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
}
