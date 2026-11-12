//! Content blocking in the page window with Brave's `adblock` engine. Filter lists are downloaded and
//! refreshed in the background, never bundled; until the first one arrives nothing is blocked.
//! Requests are answered from memory; cosmetic rules hide elements through Rust's isolated world.
//! Scriptlets and procedural rules stay off: they run code in the page.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use adblock::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use tauri::{AppHandle, Manager, Url};

use crate::platform::blocker;

/// Each list is cached under its own file name in `app_local_data_dir/content-blocking`.
const LISTS: [(&str, &str); 3] = [
    ("easylist.txt", "https://easylist.to/easylist/easylist.txt"),
    (
        "easyprivacy.txt",
        "https://easylist.to/easylist/easyprivacy.txt",
    ),
    (
        "easylist-cookie.txt",
        "https://secure.fanboy.co.nz/fanboy-cookiemonster.txt",
    ),
];
/// The compiled engine, so a start needs no parsing. Derived: rebuilt from the lists when missing,
/// corrupt or written by another `adblock` version.
const ENGINE_FILE: &str = "engine.dat";
/// EasyList and EasyPrivacy expire in four days; the cookie list in nine.
const REFRESH: Duration = Duration::from_secs(4 * 24 * 60 * 60);
/// How often the background thread looks for due lists; a failed download waits for the next look.
const CHECK: Duration = Duration::from_secs(15 * 60);
/// Lists are about 2 MB each.
const LIST_MAX_BYTES: u64 = 16 * 1024 * 1024;
const ENGINE_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Replaced whole when lists change; the WebView2 UI thread clones it per request, never waiting
/// for I/O or a build.
static ENGINE: Mutex<Option<Arc<Engine>>> = Mutex::new(None);

fn engine() -> Option<Arc<Engine>> {
    ENGINE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn set_engine(engine: Engine) {
    *ENGINE.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(engine));
}

/// Canonical site boundary from the engine's embedded public suffix list.
pub fn site_key(raw: &str) -> Option<String> {
    let mut url = Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.trim_end_matches('.').to_owned();
    url.set_host(Some(&host)).ok()?;
    if host.starts_with('[') || host.parse::<std::net::IpAddr>().is_ok() {
        return Some(host);
    }
    adblock::url_parser::parse_url(url.as_str()).map(|url| url.domain().to_owned())
}

pub fn change_allowed(sites: &mut Vec<String>, site: &str, allowed: bool) {
    sites.retain(|entry| entry != site);
    if allowed {
        sites.push(site.to_owned());
    }
    sites.sort();
    sites.dedup();
}

fn allowed(sites: &[String], source: &str) -> bool {
    !sites.is_empty() && site_key(source).is_some_and(|site| sites.contains(&site))
}

static ALLOWED: Mutex<Vec<String>> = Mutex::new(Vec::new());
// Serialize changes without holding the request-path lock during I/O.
static CHANGING: Mutex<()> = Mutex::new(());

pub fn allowed_sites() -> Vec<String> {
    ALLOWED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

#[derive(serde::Serialize)]
pub struct SiteStatus {
    site: String,
    allowed: bool,
}

pub fn status(url: &str) -> Option<SiteStatus> {
    let site = site_key(url)?;
    let allowed = ALLOWED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(&site);
    Some(SiteStatus { site, allowed })
}

pub fn set_allowed(app: &AppHandle, url: &str, enabled: bool) -> Result<(), crate::error::Error> {
    let site = site_key(url).ok_or_else(|| crate::error::Error::unexpected("not a web site"))?;
    let _change = CHANGING.lock().unwrap_or_else(PoisonError::into_inner);
    let mut sites = allowed_sites();
    change_allowed(&mut sites, &site, enabled);
    crate::notes::set_allowed_sites(app, sites.clone())?;
    *ALLOWED.lock().unwrap_or_else(PoisonError::into_inner) = sites;
    if let Some(window) = app.get_webview_window("web")
        && window
            .url()
            .ok()
            .is_some_and(|url| site_key(url.as_str()).as_ref() == Some(&site))
    {
        DOCUMENT.fetch_add(1, Ordering::SeqCst);
        window.reload()?;
    }
    Ok(())
}

/// A list is due when missing, at least `REFRESH` old, or dated in the future by a clock change.
fn due(now: SystemTime, saved: Option<SystemTime>) -> bool {
    saved.is_none_or(|saved| {
        now.duration_since(saved).is_ok_and(|age| age >= REFRESH) || saved > now
    })
}

/// A captive portal or error page would otherwise replace a list and parse as rules.
fn valid_list(text: &str) -> bool {
    text.trim_start_matches('\u{feff}')
        .starts_with("[Adblock Plus")
}

fn build(lists: Vec<String>) -> Option<Engine> {
    if lists.is_empty() {
        return None;
    }
    let mut set = FilterSet::new(false);
    for text in lists {
        set.add_filter_list(text, ParseOptions::default());
    }
    Some(Engine::new_with_filter_set(set))
}

/// The engine from its cache or, when the cache is missing or unreadable, rebuilt from the cached
/// lists (`true`). None without any list.
fn load(cache: Option<&[u8]>, lists: impl FnOnce() -> Vec<String>) -> Option<(Engine, bool)> {
    if let Some(bytes) = cache {
        let mut engine = Engine::default();
        match engine.deserialize(bytes) {
            Ok(()) => return Some((engine, false)),
            Err(error) => eprintln!("[blocking] cache: {error:?}"),
        }
    }
    build(lists()).map(|engine| (engine, true))
}

fn read_bounded(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .ok()
        .filter(|&read| read as u64 <= limit)?;
    Some(bytes)
}

fn read_lists(dir: &Path) -> Vec<String> {
    LISTS
        .iter()
        .filter_map(|(file, _)| read_bounded(&dir.join(file), LIST_MAX_BYTES))
        .filter_map(|bytes| String::from_utf8(bytes).ok())
        .filter(|text| valid_list(text))
        .collect()
}

/// HTTPS only, no cookies, bounded time and size.
fn download(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .https_only(true)
        .build()
        .into();
    let text = agent
        .get(url)
        .call()?
        .body_mut()
        .with_config()
        .limit(LIST_MAX_BYTES)
        .read_to_string()?;
    if valid_list(&text) {
        Ok(text)
    } else {
        Err("not a filter list".into())
    }
}

/// Downloads the due lists, keeping the previous copy of any that fails. True if one changed.
fn refresh(dir: &Path) -> bool {
    let now = SystemTime::now();
    let mut changed = false;
    for (file, url) in LISTS {
        let path = dir.join(file);
        let saved = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if !due(now, saved) {
            continue;
        }
        let saved = download(url).and_then(|text| {
            std::fs::create_dir_all(dir)?;
            Ok(crate::notes::write_atomic(&path, text.as_bytes())?)
        });
        match saved {
            Ok(()) => changed = true,
            Err(error) => eprintln!("[blocking] {url}: {error}"),
        }
    }
    changed
}

fn save(dir: &Path, engine: &Engine) {
    if let Err(error) = crate::notes::write_atomic(&dir.join(ENGINE_FILE), &engine.serialize()) {
        eprintln!("[blocking] save cache: {error}");
    }
}

/// Loads the cached engine, then keeps the lists fresh, on its own thread for the app's lifetime.
pub fn start(app: &AppHandle) {
    *ALLOWED.lock().unwrap_or_else(PoisonError::into_inner) = crate::notes::allowed_sites(app);
    let Ok(dir) = app.path().app_local_data_dir() else {
        return;
    };
    let dir = dir.join("content-blocking");
    let spawned = std::thread::Builder::new()
        .name("blocking".into())
        .spawn(move || {
            // The file's bytes go before the thread settles into its loop.
            let loaded = {
                let cache = read_bounded(&dir.join(ENGINE_FILE), ENGINE_MAX_BYTES);
                load(cache.as_deref(), || read_lists(&dir))
            };
            if let Some((engine, rebuilt)) = loaded {
                if rebuilt {
                    save(&dir, &engine);
                }
                set_engine(engine);
            }
            loop {
                if refresh(&dir)
                    && let Some(engine) = build(read_lists(&dir))
                {
                    save(&dir, &engine);
                    set_engine(engine);
                }
                std::thread::sleep(CHECK);
            }
        });
    if let Err(error) = spawned {
        eprintln!("[blocking] {error}");
    }
}

/// The page the user asked for always loads; only what it pulls in is filtered. URLs the engine
/// cannot parse, such as `data:` and `blob:`, pass.
fn blocks(engine: &Engine, sites: &[String], request: &blocker::Request) -> bool {
    // The engine decides first: parsing the page's site is only needed when it would block.
    request.kind != "document"
        && adblock::request::Request::new(request.url, request.source, request.kind, request.method)
            .is_ok_and(|request| engine.check_network_request(&request).should_block())
        && !allowed(sites, request.source)
}

/// Runs on the WebView2 UI thread for every request of the page window.
pub fn blocked(request: &blocker::Request) -> bool {
    engine().is_some_and(|engine| {
        blocks(
            &engine,
            &ALLOWED.lock().unwrap_or_else(PoisonError::into_inner),
            request,
        )
    })
}

/// A selector goes into `<selector>{display:none!important}`: braces would let a list add other
/// declarations or rules, and an at-rule is not a selector.
fn safe_selector(selector: &str) -> bool {
    !selector.is_empty() && !selector.contains(['{', '}']) && !selector.starts_with('@')
}

/// Delays between the passes over one document. The first hides what applies to the host before
/// the page renders and starts watching classes and ids; the next hide generic rules for those seen
/// so far, including banners that scripts add after load. The last stops watching.
const PASSES: [Duration; 6] = [
    Duration::ZERO,
    Duration::ZERO,
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
];
/// Bounds what the page can make a pass carry back.
const SEEN_MAX_JSON: usize = 2 * 1024 * 1024;
/// Called as `(host, selectors, last)` in Rust's isolated world, with JSON arguments. Its builtins and
/// DOM wrappers are the world's own, so the page cannot change what it does or returns; the page
/// can only see the sheet and remove it from `adoptedStyleSheets`, and a later pass adds it back.
/// An adopted sheet adds no node that hydration could trip on. `insertRule` takes one rule at a
/// time, so an invalid selector drops only itself. It returns up to 1,000 new class names and ids
/// of at most 100 characters each, and stops recording after 20,000.
const COSMETIC_SCRIPT: &str = "((host, hide, last) => {
  if (location.hostname !== host) return null;
  let state = globalThis.docdockHide;
  if (!state) {
    const fresh = [new Set(), new Set()];
    const seen = new Set();
    const add = (kind, name) => {
      const key = kind + name;
      if (name.length <= 100 && seen.size < 20000 && !seen.has(key)) {
        seen.add(key);
        fresh[kind].add(name);
      }
    };
    const note = (element) => {
      for (const name of element.classList) add(0, name);
      if (typeof element.id === 'string' && element.id) add(1, element.id);
    };
    const scan = (node) => {
      if (node.nodeType !== 1) return;
      note(node);
      for (const element of node.querySelectorAll('[class],[id]')) note(element);
    };
    const observer = new MutationObserver((records) => {
      for (const record of records) {
        if (record.type === 'attributes') note(record.target);
        else for (const node of record.addedNodes) scan(node);
      }
    });
    observer.observe(document, { childList: true, subtree: true, attributes: true,
      attributeFilter: ['class', 'id'] });
    if (document.documentElement) scan(document.documentElement);
    state = globalThis.docdockHide = { sheet: new CSSStyleSheet(), fresh, observer };
  }
  const sheet = state.sheet;
  if (!document.adoptedStyleSheets.includes(sheet)) {
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
  }
  for (const selector of hide) {
    try { sheet.insertRule(selector + '{display:none!important}', sheet.cssRules.length); } catch {}
  }
  if (last) state.observer.disconnect();
  const take = (names) => {
    const taken = [...names].slice(0, 1000);
    for (const name of taken) names.delete(name);
    return taken;
  };
  return [take(state.fresh[0]), take(state.fresh[1])];
})";

#[derive(serde::Deserialize)]
struct Evaluated {
    result: Remote,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Remote {
    /// Class names, then ids.
    Object { value: (Vec<String>, Vec<String>) },
    #[serde(other)]
    Other,
}

/// New class names and ids from a pass's `Runtime.evaluate` response; None for anything else,
/// including the null of a document on another host.
fn decode_seen(json: &str) -> Option<(Vec<String>, Vec<String>)> {
    if json.len() > SEEN_MAX_JSON {
        return None;
    }
    match serde_json::from_str(json) {
        Ok(Evaluated {
            result: Remote::Object { value },
        }) => Some(value),
        _ => None,
    }
}

/// Changes with each main-frame document, so its passes stop when the next one commits.
static DOCUMENT: AtomicU64 = AtomicU64::new(0);

/// Called on the WebView2 UI thread when a main-frame document commits; the passes run elsewhere.
pub fn committed(app: &AppHandle, url: String) {
    let document = DOCUMENT.fetch_add(1, Ordering::SeqCst) + 1;
    if allowed(
        &ALLOWED.lock().unwrap_or_else(PoisonError::into_inner),
        &url,
    ) {
        return;
    }
    let Some(engine) = engine() else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || hide(&app, &engine, document, &url));
}

fn hide(app: &AppHandle, engine: &Engine, document: u64, url: &str) {
    let Some(host) = Url::parse(url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .and_then(|url| url.host_str().map(str::to_owned))
    else {
        return;
    };
    let resources = engine.url_cosmetic_resources(url);
    let mut shown = HashSet::new();
    let mut pending: Vec<String> = resources.hide_selectors.into_iter().collect();
    for (index, delay) in PASSES.into_iter().enumerate() {
        std::thread::sleep(delay);
        if DOCUMENT.load(Ordering::SeqCst) != document {
            return;
        }
        pending.retain(|selector| safe_selector(selector) && !shown.contains(selector));
        let arguments = serde_json::json!([host, pending, index + 1 == PASSES.len()]);
        let expression = format!("{COSMETIC_SCRIPT}(...{arguments})");
        // A pass that gets no answer keeps its selectors for the next one.
        let Some((classes, ids)) =
            crate::web::evaluate(app, expression).and_then(|json| decode_seen(&json))
        else {
            continue;
        };
        shown.extend(pending.drain(..));
        if !resources.generichide {
            pending = engine.hidden_class_id_selectors(
                classes.iter().take(1000),
                ids.iter().take(1000),
                &resources.exceptions,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn list(rules: &str) -> String {
        format!("[Adblock Plus 2.0]\n! Title: Test\n{rules}\n")
    }

    #[test]
    fn exceptions_use_registrable_domains_not_suffix_matches() {
        let sites = vec!["example.com".into(), "example.co.uk".into()];
        assert!(allowed(&sites, "https://news.example.co.uk/a"));
        assert!(allowed(&sites, "https://NEWS.Example.com.:8443/a"));
        assert!(!allowed(&sites, "https://evil-example.com/"));
        assert!(!allowed(&sites, "https://example.com.evil.net/"));
        assert!(!allowed(&sites, "https://example.com@evil.net/"));
    }

    #[test]
    fn local_hosts_and_ip_addresses_are_exact_sites() {
        for (url, site) in [
            ("http://localhost:3000/a", "localhost"),
            ("http://127.0.0.1:8080/", "127.0.0.1"),
            ("http://[::1]:8080/", "[::1]"),
        ] {
            assert_eq!(site_key(url).as_deref(), Some(site));
        }
        assert!(!allowed(&["localhost".into()], "http://other.localhost/"));
        assert!(site_key("file:///example.com").is_none());
        assert!(site_key("about:blank").is_none());
    }

    #[test]
    fn private_suffix_tenants_do_not_share_exceptions() {
        assert!(!allowed(
            &["alice.github.io".into()],
            "https://bob.github.io/"
        ));
        assert_eq!(
            site_key("https://www.alice.github.io/").as_deref(),
            Some("alice.github.io")
        );
    }

    #[test]
    fn exception_changes_are_deduplicated_and_removable() {
        let mut sites = Vec::new();
        change_allowed(&mut sites, "example.com", true);
        change_allowed(&mut sites, "example.com", true);
        assert_eq!(sites, ["example.com"]);
        change_allowed(&mut sites, "example.com", false);
        assert!(sites.is_empty());
    }

    #[test]
    fn third_party_requests_follow_the_top_level_site() {
        let sites = vec!["example.com".into()];
        let request = blocker::Request {
            url: "https://ads.other.net/ad.js",
            source: "https://news.example.com/",
            kind: "script",
            method: "GET",
        };
        let engine = build(vec![list("||ads.other.net^")]).unwrap();
        assert!(blocks(&engine, &[], &request));
        assert!(!blocks(&engine, &sites, &request));
        let request = blocker::Request {
            source: "https://unrelated.net/",
            ..request
        };
        assert!(blocks(&engine, &sites, &request));
    }

    #[test]
    fn a_missing_list_is_due() {
        assert!(due(SystemTime::now(), None));
    }

    #[test]
    fn a_list_is_due_once_its_refresh_interval_passes() {
        let saved = SystemTime::UNIX_EPOCH + 1000 * DAY;
        assert!(!due(saved + REFRESH - Duration::from_secs(1), Some(saved)));
        assert!(due(saved + REFRESH, Some(saved)));
    }

    #[test]
    fn a_list_dated_in_the_future_is_due() {
        let now = SystemTime::UNIX_EPOCH + 1000 * DAY;
        assert!(due(now, Some(now + DAY)));
    }

    #[test]
    fn an_error_page_is_not_a_list() {
        assert!(!valid_list(
            "<!DOCTYPE html><html><body>Sign in to Wi-Fi</body></html>"
        ));
        assert!(valid_list(&format!("\u{feff}{}", list("||ads.example^"))));
    }

    #[test]
    fn a_corrupt_cache_is_rebuilt_from_the_lists() {
        let (engine, rebuilt) = load(Some(b"corrupt"), || vec![list("||ads.example^")]).unwrap();
        assert!(rebuilt);
        let request = blocker::Request {
            url: "https://ads.example/a.js",
            source: "https://news.example/",
            kind: "script",
            method: "GET",
        };
        assert!(blocks(&engine, &[], &request));
    }

    #[test]
    fn a_corrupt_cache_without_lists_blocks_nothing() {
        assert!(load(Some(b"corrupt"), Vec::new).is_none());
    }

    #[test]
    fn the_page_asked_for_is_never_blocked() {
        let engine = build(vec![list("||ads.example^")]).unwrap();
        let request = blocker::Request {
            url: "https://ads.example/",
            source: "https://ads.example/",
            kind: "document",
            method: "GET",
        };
        assert!(!blocks(&engine, &[], &request));
    }

    #[test]
    fn a_list_selector_cannot_add_declarations() {
        assert!(safe_selector(".cookie-banner > div[id^=\"ad\"]"));
        for selector in [
            "a{background:url(//x.example)}",
            "a}b",
            "@import url(x)",
            "",
        ] {
            assert!(!safe_selector(selector), "{selector}");
        }
    }
}
