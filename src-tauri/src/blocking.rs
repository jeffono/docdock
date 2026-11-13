//! Content blocking in the page window with Brave's `adblock` engine. Filter lists are downloaded and
//! refreshed in the background, never bundled; until the first one arrives nothing is blocked.
//! Requests are answered from memory; cosmetic rules hide elements through Rust's isolated world.
//! Scriptlets and procedural rules stay off: they run code in the page.

use std::collections::{BTreeSet, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
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
/// The compiled engine, so a start needs no parsing, after the fingerprint of the lists it was built
/// from. Derived: rebuilt from the lists when missing, corrupt, written by another `adblock` version
/// or built from other lists.
const ENGINE_FILE: &str = "engine.dat";
/// EasyList and EasyPrivacy expire in four days; the cookie list in nine.
const REFRESH: Duration = Duration::from_secs(4 * 24 * 60 * 60);
/// How often the background thread looks for due lists; a failed download waits for the next look.
const CHECK: Duration = Duration::from_secs(15 * 60);
/// Lists are about 2 MB each.
const LIST_MAX_BYTES: u64 = 16 * 1024 * 1024;
const ENGINE_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// The engine and what is derived from it for document starts.
struct Filters {
    engine: Engine,
    /// `generic_rules` as a JavaScript string literal, ready for each document start.
    generic: String,
}

/// Replaced whole when lists change; the WebView2 UI thread clones it per request, never waiting
/// for I/O or a build.
static FILTERS: Mutex<Option<Arc<Filters>>> = Mutex::new(None);

fn filters() -> Option<Arc<Filters>> {
    FILTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

impl Filters {
    fn new(engine: Engine, keys: &BTreeSet<String>) -> Self {
        let generic = serde_json::to_string(&generic_rules(&engine, keys)).unwrap_or_default();
        Self { engine, generic }
    }
}

fn set_filters(filters: Filters) {
    *FILTERS.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(filters));
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
        && let Ok(url) = window.url()
        && site_key(url.as_str()).as_ref() == Some(&site)
    {
        DOCUMENT.fetch_add(1, Ordering::SeqCst);
        blocker::prepare(&window, starting(url.as_str()))?;
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

/// Identifies the lists an engine is built from. A mismatch only costs a rebuild, so the standard
/// hasher, whose algorithm may change between Rust releases, is enough.
fn fingerprint(lists: &[String]) -> [u8; 8] {
    let mut hasher = DefaultHasher::new();
    lists.hash(&mut hasher);
    hasher.finish().to_le_bytes()
}

fn cache_bytes(key: &[u8; 8], engine: &Engine) -> Vec<u8> {
    [key.as_slice(), &engine.serialize()].concat()
}

/// The engine from its cache when the cache was built from the lists with fingerprint `key`;
/// otherwise rebuilt from `lists` (`true`). Lists are replaced before the cache, so a crash or a
/// failed cache write in between leaves a cache that no longer matches. None without any list.
fn load(cache: Option<&[u8]>, key: &[u8; 8], lists: Vec<String>) -> Option<(Engine, bool)> {
    match cache.map(|bytes| bytes.strip_prefix(key.as_slice())) {
        Some(Some(bytes)) => {
            let mut engine = Engine::default();
            match engine.deserialize(bytes) {
                Ok(()) => return Some((engine, false)),
                Err(error) => eprintln!("[blocking] cache: {error:?}"),
            }
        }
        Some(None) => eprintln!("[blocking] cache: built from other lists"),
        None => {}
    }
    build(lists).map(|engine| (engine, true))
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

/// Installs the engine for the cached lists: from `cache`, the engine file's bytes, when it matches
/// them, otherwise built and saved. Keeps the current engine without any list.
fn install(dir: &Path, cache: Option<&[u8]>) {
    let lists = read_lists(dir);
    let key = fingerprint(&lists);
    let keys = rule_keys(&lists);
    let Some((engine, rebuilt)) = load(cache, &key, lists) else {
        return;
    };
    if rebuilt
        && let Err(error) =
            crate::notes::write_atomic(&dir.join(ENGINE_FILE), &cache_bytes(&key, &engine))
    {
        eprintln!("[blocking] save cache: {error}");
    }
    set_filters(Filters::new(engine, &keys));
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
            install(
                &dir,
                read_bounded(&dir.join(ENGINE_FILE), ENGINE_MAX_BYTES).as_deref(),
            );
            loop {
                if refresh(&dir) {
                    install(&dir, None);
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
    filters().is_some_and(|filters| {
        blocks(
            &filters.engine,
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

/// Classes and ids, as `.name` and `#name`, that lead some element hiding selector of the lists:
/// every key the engine can hold generic rules for, and host-specific keys that it then ignores.
fn rule_keys(lists: &[String]) -> BTreeSet<String> {
    let name = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_');
    lists
        .iter()
        .flat_map(|list| list.lines())
        .filter_map(|line| line.split_once("##").map(|(_, selector)| selector))
        .filter(|selector| selector.starts_with(['.', '#']))
        .map(|selector| {
            let end = selector[1..]
                .find(|c| !name(c))
                .map_or(selector.len(), |i| i + 1);
            selector[..end].to_owned()
        })
        .filter(|key| key.len() > 1)
        .collect()
}

/// The engine's generic class and id rules, one key per line followed by its selectors, separated
/// by tabs; a key alone hides itself. The engine decides what is generic, so host-specific keys
/// stay out, and the page can apply these rules itself as elements appear.
fn generic_rules(engine: &Engine, keys: &BTreeSet<String>) -> String {
    let none = HashSet::new();
    let mut rules = String::new();
    for key in keys {
        let name = [&key[1..]];
        let selectors = if key.starts_with('.') {
            engine.hidden_class_id_selectors(name, [] as [&str; 0], &none)
        } else {
            engine.hidden_class_id_selectors([] as [&str; 0], name, &none)
        };
        let selectors: Vec<String> = selectors
            .into_iter()
            .filter(|selector| safe_selector(selector) && !selector.contains(['\t', '\n']))
            .collect();
        if selectors.is_empty() {
            continue;
        }
        rules.push_str(key);
        if selectors != [key.as_str()] {
            for selector in &selectors {
                rules.push('\t');
                rules.push_str(selector);
            }
        }
        rules.push('\n');
    }
    rules.pop();
    rules
}

/// Builds the hiding state of one document in Rust's isolated world, called as `(rules, skip)`:
/// a map from `.class` or `#id` to the selectors it brings, and the selectors the host excepts.
/// Its builtins and DOM wrappers are the world's own, so the page cannot change what it does; the
/// page can only see the sheet and remove it from `adoptedStyleSheets`, which the observer and the
/// next pass undo. An adopted sheet adds no node that hydration could trip on. `insertRule` takes
/// one rule at a time, so an invalid selector drops only itself. A `MutationObserver` applies the
/// rules of each class and id as they appear, in the microtask after the change and so before the
/// next paint, and records up to 20,000 new names of at most 100 characters for the passes until
/// `stop`. Without rules, `stop` also disconnects it.
const STATE_SCRIPT: &str = "((rules, skip) => {
  const sheet = new CSSStyleSheet();
  const hide = (selector) => {
    try { sheet.insertRule(selector + '{display:none!important}', sheet.cssRules.length); } catch {}
  };
  const adopt = () => {
    if (!document.adoptedStyleSheets.includes(sheet)) {
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
    }
  };
  const fresh = [new Set(), new Set()];
  const seen = new Set();
  let recording = true;
  const add = (kind, name) => {
    const key = (kind ? '#' : '.') + name;
    const selectors = rules.get(key);
    if (selectors) {
      rules.delete(key);
      for (const selector of selectors) if (!skip.has(selector)) hide(selector);
    }
    if (recording && name.length <= 100 && seen.size < 20000 && !seen.has(key)) {
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
    if (rules.size) adopt();
  });
  observer.observe(document, { childList: true, subtree: true, attributes: true,
    attributeFilter: ['class', 'id'] });
  if (document.documentElement) scan(document.documentElement);
  const stop = () => {
    recording = false;
    if (!rules.size) observer.disconnect();
  };
  return { hide, adopt, fresh, stop };
})";

/// Runs in Rust's isolated world when a main-frame document starts, before its own scripts and
/// first paint, called as `(state, host, hide, skip, generic)`: `generic` is the text of
/// `generic_rules`, or null under `$generichide`. Frames and documents of other hosts are left
/// alone.
const START_SCRIPT: &str = "((state, host, hide, skip, generic) => {
  if (window !== window.top || location.hostname !== host || globalThis.docdockHide) return;
  const rules = new Map();
  for (const line of generic ? generic.split('\\n') : []) {
    const [key, ...selectors] = line.split('\\t');
    rules.set(key, selectors.length ? selectors : [key]);
  }
  const hiding = globalThis.docdockHide = state(rules, new Set(skip));
  for (const selector of hide) hiding.hide(selector);
  hiding.adopt();
})";

/// The document-start script for a main-frame navigation to `url`: the host's selectors and the
/// generic rules, minus the host's exceptions. None for an allowed site or anything but HTTP(S).
fn start_script(filters: &Filters, sites: &[String], url: &str) -> Option<String> {
    if allowed(sites, url) {
        return None;
    }
    let host = web_host(url)?;
    let resources = filters.engine.url_cosmetic_resources(url);
    let hide: Vec<&String> = resources
        .hide_selectors
        .iter()
        .filter(|selector| safe_selector(selector))
        .collect();
    let host = serde_json::to_string(&host).ok()?;
    let hide = serde_json::to_string(&hide).ok()?;
    let skip = serde_json::to_string(&resources.exceptions).ok()?;
    let generic = if resources.generichide {
        "null"
    } else {
        &filters.generic
    };
    Some(format!(
        "{START_SCRIPT}({STATE_SCRIPT},{host},{hide},{skip},{generic})"
    ))
}

/// Called on the WebView2 UI thread when a main-frame navigation starts, so it must be quick: the
/// engine answers from memory.
pub fn starting(url: &str) -> Option<String> {
    let filters = filters()?;
    start_script(
        &filters,
        &ALLOWED.lock().unwrap_or_else(PoisonError::into_inner),
        url,
    )
}

fn web_host(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .and_then(|url| url.host_str().map(str::to_owned))
}

/// Delays between the passes over one document. The document-start script normally hides
/// everything already; the passes cover a document that started before it was registered. The
/// first hides what applies to the host and starts watching classes and ids; the next hide generic
/// rules for those seen so far, including banners that scripts add after load. The last stops
/// recording.
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
/// Called as `(state, host, selectors, last)` in Rust's isolated world, with JSON arguments, on the
/// document-start state when there is one. It returns up to 1,000 new class names and ids.
const COSMETIC_SCRIPT: &str = "((state, host, hide, last) => {
  if (location.hostname !== host) return null;
  const hiding = globalThis.docdockHide ??= state(new Map(), new Set());
  hiding.adopt();
  for (const selector of hide) hiding.hide(selector);
  if (last) hiding.stop();
  const take = (names) => {
    const taken = [...names].slice(0, 1000);
    for (const name of taken) names.delete(name);
    return taken;
  };
  return [take(hiding.fresh[0]), take(hiding.fresh[1])];
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
    let Some(filters) = filters() else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || hide(&app, &filters.engine, document, &url));
}

fn hide(app: &AppHandle, engine: &Engine, document: u64, url: &str) {
    let Some(host) = web_host(url) else {
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
        let expression = format!("{COSMETIC_SCRIPT}({STATE_SCRIPT}, ...{arguments})");
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

    fn script(url: &str) -> blocker::Request<'_> {
        blocker::Request {
            url,
            source: "https://news.example/",
            kind: "script",
            method: "GET",
        }
    }

    /// The cache of an engine built from `lists`, and its own lists' fingerprint.
    fn cached(lists: &[String]) -> Vec<u8> {
        cache_bytes(&fingerprint(lists), &build(lists.to_vec()).unwrap())
    }

    #[test]
    fn a_corrupt_cache_is_rebuilt_from_the_lists() {
        let lists = vec![list("||ads.example^")];
        let mut cache = fingerprint(&lists).to_vec();
        cache.extend(b"corrupt");
        let (engine, rebuilt) = load(Some(&cache), &fingerprint(&lists), lists).unwrap();
        assert!(rebuilt);
        assert!(blocks(&engine, &[], &script("https://ads.example/a.js")));
    }

    #[test]
    fn a_corrupt_cache_without_lists_blocks_nothing() {
        assert!(load(Some(b"corrupt"), &fingerprint(&[]), Vec::new()).is_none());
    }

    /// Lists replaced before a crash or a failed cache write must not keep the old engine.
    #[test]
    fn a_cache_built_from_other_lists_is_rebuilt() {
        let cache = cached(&[list("||old.example^")]);
        let lists = vec![list("||new.example^")];
        let (engine, rebuilt) = load(Some(&cache), &fingerprint(&lists), lists).unwrap();
        assert!(rebuilt);
        assert!(blocks(&engine, &[], &script("https://new.example/a.js")));
    }

    #[test]
    fn a_cache_built_from_the_same_lists_is_used() {
        let lists = vec![list("||ads.example^")];
        let cache = cached(&lists);
        let (_, rebuilt) = load(Some(&cache), &fingerprint(&lists), lists).unwrap();
        assert!(!rebuilt);
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

    fn filters(rules: &str) -> Filters {
        let lists = vec![list(rules)];
        let keys = rule_keys(&lists);
        Filters::new(build(lists).unwrap(), &keys)
    }

    /// The g1 cookie banner is hidden by `###cookie-banner-lgpd`, a generic rule.
    #[test]
    fn document_starts_carry_generic_class_and_id_rules() {
        let filters = filters(
            "###cookie-banner-lgpd
##.ad-box
##.promo > .banner",
        );
        let rules: String = serde_json::from_str(&filters.generic).unwrap();
        let rules: Vec<&str> = rules.lines().collect();
        assert_eq!(
            rules,
            ["#cookie-banner-lgpd", ".ad-box", ".promo	.promo > .banner"]
        );
    }

    #[test]
    fn host_specific_rules_stay_out_of_generic_rules() {
        let filters = filters(
            "example.com##.local-only
example.com###local-id",
        );
        assert_eq!(filters.generic, "\"\"");
        let script = start_script(&filters, &[], "https://example.com/").unwrap();
        assert!(script.contains("\".local-only\"") && script.contains("\"#local-id\""));
    }

    #[test]
    fn an_allowed_site_gets_no_document_start_hiding() {
        let filters = filters("##.ad-box");
        let sites = vec!["example.com".into()];
        assert!(start_script(&filters, &sites, "https://news.example.com/").is_none());
        assert!(start_script(&filters, &sites, "https://other.net/").is_some());
    }

    #[test]
    fn a_sites_exceptions_reach_its_document_start() {
        let filters = filters(
            "##.ad-box
example.com#@#.ad-box",
        );
        let script = start_script(&filters, &[], "https://example.com/").unwrap();
        assert!(script.ends_with(r#",[".ad-box"],".ad-box")"#), "{script}");
    }

    #[test]
    fn generichide_drops_generic_rules_at_document_start() {
        let filters = filters(
            "##.ad-box
@@||example.com^$generichide",
        );
        let script = start_script(&filters, &[], "https://example.com/").unwrap();
        assert!(script.ends_with(",null)"), "{script}");
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
