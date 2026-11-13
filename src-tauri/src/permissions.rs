//! Site permission requests of the page window (location, camera, microphone, clipboard), asked in
//! the panel instead of WebView2's own prompt, which takes focus and appears away from the panel.
//! Answers are remembered per registrable site and kind in app preferences. Open requests wait, with
//! their WebView2 deferrals, on the WebView2 UI thread.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError, mpsc};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Url};

use crate::blocking::site_key;
use crate::error::Error;
use crate::platform::permission::{Asked, Held};

/// The kinds the panel asks about. Every other WebView2 kind is denied without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Location,
    Camera,
    Microphone,
    ClipboardRead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remembered {
    pub site: String,
    pub kind: Kind,
    pub decision: Decision,
}

/// What the bar asks. `origin`: the asking frame's, only when it is not the page's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Prompt {
    pub site: String,
    pub kind: Kind,
    pub origin: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Allow,
    Deny,
    Ask(Prompt),
}

pub fn lookup(remembered: &[Remembered], site: &str, kind: Kind) -> Option<Decision> {
    remembered
        .iter()
        .find(|entry| entry.site == site && entry.kind == kind)
        .map(|entry| entry.decision)
}

/// Replaces the choice for `site` and `kind`, or forgets it with `None`.
pub fn remember(list: &mut Vec<Remembered>, site: &str, kind: Kind, decision: Option<Decision>) {
    list.retain(|entry| entry.site != site || entry.kind != kind);
    if let Some(decision) = decision {
        list.push(Remembered {
            site: site.to_owned(),
            kind,
            decision,
        });
    }
    list.sort_by(|a, b| (&a.site, a.kind).cmp(&(&b.site, b.kind)));
}

/// Choices read from settings: each entry stands alone, a site that is not already a site key would
/// widen or move its scope, and of two choices for the same site and kind a block wins.
pub fn from_settings(entries: Vec<serde_json::Value>) -> Vec<Remembered> {
    let mut list = Vec::new();
    for entry in entries {
        let Ok(entry) = serde_json::from_value::<Remembered>(entry) else {
            continue;
        };
        if site_key(&format!("https://{}/", entry.site)).as_deref() == Some(entry.site.as_str())
            && lookup(&list, &entry.site, entry.kind) != Some(Decision::Block)
        {
            remember(&mut list, &entry.site, entry.kind, Some(entry.decision));
        }
    }
    list
}

/// The asking frame's origin when it differs from the page's.
fn other_origin(origin: &str, top: &str) -> Option<String> {
    let origin = Url::parse(origin).ok()?.origin();
    let top = Url::parse(top).ok().map(|top| top.origin());
    (top.as_ref() != Some(&origin)).then(|| origin.ascii_serialization())
}

/// Decides a request from `origin`, a frame of the page at `top`, for `kind` (`None`: a kind the panel
/// never asks about). The page's site decides; a frame of another site never uses or asks for the
/// page's permissions. Without a remembered choice, only a page on screen (`shown`) asks.
pub fn decide(
    remembered: &[Remembered],
    shown: bool,
    kind: Option<Kind>,
    origin: &str,
    top: &str,
) -> Outcome {
    let (Some(kind), Some(site)) = (kind, site_key(top)) else {
        return Outcome::Deny;
    };
    if site_key(origin).as_deref() != Some(site.as_str()) {
        return Outcome::Deny;
    }
    match lookup(remembered, &site, kind) {
        Some(Decision::Allow) => Outcome::Allow,
        Some(Decision::Block) => Outcome::Deny,
        None if shown => Outcome::Ask(Prompt {
            origin: other_origin(origin, top),
            site,
            kind,
        }),
        None => Outcome::Deny,
    }
}

/// A page cannot pile up requests: beyond this many waiting, more are denied.
const WAITING_MAX: usize = 8;

/// Requests waiting for the bar, oldest first; the bar shows the first.
struct Queue<T> {
    next: u64,
    waiting: VecDeque<(u64, Prompt, T)>,
}

impl<T> Queue<T> {
    const fn new() -> Self {
        Self {
            next: 0,
            waiting: VecDeque::new(),
        }
    }

    /// Gives `held` back when full.
    fn push(&mut self, prompt: Prompt, held: T) -> Result<(), T> {
        if self.waiting.len() >= WAITING_MAX {
            return Err(held);
        }
        self.next += 1;
        self.waiting.push_back((self.next, prompt, held));
        Ok(())
    }

    fn first(&self) -> Option<(u64, &Prompt)> {
        self.waiting.front().map(|(id, prompt, _)| (*id, prompt))
    }

    /// Removes request `id` and every other one the same answer settles: the same site and kind.
    /// None when it no longer waits.
    fn answer(&mut self, id: u64) -> Option<(Prompt, Vec<T>)> {
        let prompt = self
            .waiting
            .iter()
            .find(|(waiting, ..)| *waiting == id)
            .map(|(_, prompt, _)| prompt.clone())?;
        let (settled, kept) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|(_, other, _)| other.site == prompt.site && other.kind == prompt.kind);
        self.waiting = kept;
        Some((prompt, settled.into_iter().map(|(.., held)| held).collect()))
    }

    fn clear(&mut self) -> Vec<T> {
        self.waiting.drain(..).map(|(.., held)| held).collect()
    }
}

/// Memory copy of the saved choices; the UI thread reads it for every request, never the disk.
static REMEMBERED: Mutex<Vec<Remembered>> = Mutex::new(Vec::new());
/// Serializes changes without holding the request-path lock during I/O.
static CHANGING: Mutex<()> = Mutex::new(());
/// The page window is on screen: the panel is open and shows it.
static SHOWN: AtomicBool = AtomicBool::new(false);
/// A request waits for the bar; the panel stays open meanwhile.
static ASKING: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Only on the WebView2 UI thread, which owns the deferrals.
    static WAITING: RefCell<Queue<Held>> = const { RefCell::new(Queue::new()) };
}

pub fn start(app: &AppHandle) {
    *REMEMBERED.lock().unwrap_or_else(PoisonError::into_inner) =
        crate::notes::site_permissions(app);
}

pub fn remembered() -> Vec<Remembered> {
    REMEMBERED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Whether the panel must stay open: a request waits for an answer.
pub fn asking() -> bool {
    ASKING.load(Ordering::SeqCst)
}

#[derive(Clone, Serialize)]
struct Shown<'a> {
    id: u64,
    #[serde(flatten)]
    prompt: &'a Prompt,
}

/// Tells the UI which request the bar shows, if any. UI thread.
fn publish(app: &AppHandle) {
    WAITING.with_borrow(|queue| {
        let first = queue.first();
        ASKING.store(first.is_some(), Ordering::SeqCst);
        let shown = first.map(|(id, prompt)| Shown { id, prompt });
        let _ = app.emit_to("main", "permission", shown);
    });
}

/// Denies, without remembering, every waiting request. UI thread.
fn deny_waiting(app: &AppHandle) {
    let held = WAITING.with_borrow_mut(Queue::clear);
    if held.is_empty() {
        return;
    }
    for held in held {
        held.settle(false);
    }
    publish(app);
}

/// A request of the page window. Runs on the WebView2 UI thread, so it must return at once.
pub fn requested(app: &AppHandle, asked: Asked, held: Held) {
    let outcome = decide(
        &REMEMBERED.lock().unwrap_or_else(PoisonError::into_inner),
        SHOWN.load(Ordering::SeqCst),
        asked.kind,
        &asked.origin,
        &asked.top,
    );
    match outcome {
        Outcome::Allow => held.settle(true),
        Outcome::Deny => held.settle(false),
        Outcome::Ask(prompt) => {
            if let Err(held) = WAITING.with_borrow_mut(|queue| queue.push(prompt, held)) {
                held.settle(false);
            }
            publish(app);
        }
    }
}

/// A new main-frame document replaced the one whose requests wait. UI thread.
pub fn committed(app: &AppHandle) {
    deny_waiting(app);
}

/// From the panel thread: whether the page is on screen. Hiding the panel or the page denies, without
/// remembering, whatever waits: an answer comes only from a bar the user can see.
pub fn set_shown(app: &AppHandle, shown: bool) {
    if SHOWN.swap(shown, Ordering::SeqCst) && !shown {
        let handle = app.clone();
        if let Err(error) = app.run_on_main_thread(move || deny_waiting(&handle)) {
            eprintln!("[permissions] {error}");
        }
    }
}

/// Answers the bar's request `id`, and the others waiting for the same site and kind, then remembers
/// the choice. Waits for the WebView2 UI thread, so never call it from that thread.
pub fn answer(app: &AppHandle, id: u64, allow: bool) -> Result<(), Error> {
    let (sender, receiver) = mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let Some((prompt, held)) = WAITING.with_borrow_mut(|queue| queue.answer(id)) else {
            return;
        };
        for held in held {
            held.settle(allow);
        }
        publish(&handle);
        let _ = sender.send(prompt);
    })?;
    // No longer waiting, as after a navigation: nothing to remember.
    let Ok(prompt) = receiver.recv() else {
        return Ok(());
    };
    let decision = if allow {
        Decision::Allow
    } else {
        Decision::Block
    };
    change(app, &prompt.site, prompt.kind, Some(decision))
}

/// Forgets a choice: the site asks again.
pub fn forget(app: &AppHandle, site: &str, kind: Kind) -> Result<(), Error> {
    change(app, site, kind, None)
}

/// Saves first and publishes to memory only after a successful save.
fn change(
    app: &AppHandle,
    site: &str,
    kind: Kind,
    decision: Option<Decision>,
) -> Result<(), Error> {
    let _change = CHANGING.lock().unwrap_or_else(PoisonError::into_inner);
    let mut list = remembered();
    remember(&mut list, site, kind, decision);
    crate::notes::set_site_permissions(app, list.clone())?;
    *REMEMBERED.lock().unwrap_or_else(PoisonError::into_inner) = list;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(site: &str, kind: Kind, decision: Decision) -> Remembered {
        Remembered {
            site: site.into(),
            kind,
            decision,
        }
    }

    /// A request from the page's own main frame.
    fn page(remembered: &[Remembered], kind: Kind, url: &str) -> Outcome {
        decide(remembered, true, Some(kind), url, url)
    }

    fn prompt(site: &str, kind: Kind) -> Prompt {
        Prompt {
            site: site.into(),
            kind,
            origin: None,
        }
    }

    #[test]
    fn a_choice_applies_to_its_site_and_subdomains_only() {
        let list = [entry("example.com", Kind::Location, Decision::Allow)];
        assert_eq!(
            page(&list, Kind::Location, "https://maps.example.com/a"),
            Outcome::Allow
        );
        for url in [
            "https://evil-example.com/",
            "https://example.com.evil.net/",
            "https://example.com@evil.net/",
        ] {
            assert!(
                matches!(page(&list, Kind::Location, url), Outcome::Ask(_)),
                "{url}"
            );
        }
    }

    #[test]
    fn a_choice_applies_to_its_kind_only() {
        let list = [entry("example.com", Kind::Camera, Decision::Allow)];
        assert!(matches!(
            page(&list, Kind::Microphone, "https://example.com/"),
            Outcome::Ask(_)
        ));
    }

    #[test]
    fn a_block_is_never_turned_into_an_allow() {
        let list = [
            entry("example.com", Kind::Location, Decision::Block),
            entry("example.com", Kind::Camera, Decision::Allow),
            entry("other.net", Kind::Location, Decision::Allow),
        ];
        for (shown, origin) in [
            (true, "https://example.com/"),
            (false, "https://example.com/"),
            (true, "https://maps.example.com/"),
        ] {
            assert_eq!(
                decide(
                    &list,
                    shown,
                    Some(Kind::Location),
                    origin,
                    "https://www.example.com/"
                ),
                Outcome::Deny
            );
        }
    }

    #[test]
    fn a_frame_of_another_site_is_denied_even_where_the_page_is_allowed() {
        let list = [entry("example.com", Kind::Location, Decision::Allow)];
        for origin in ["https://ads.net/", "about:blank", ""] {
            assert_eq!(
                decide(
                    &list,
                    true,
                    Some(Kind::Location),
                    origin,
                    "https://example.com/"
                ),
                Outcome::Deny,
                "{origin}"
            );
        }
    }

    #[test]
    fn a_frame_of_the_same_site_asks_for_the_site_and_names_its_origin() {
        let outcome = decide(
            &[],
            true,
            Some(Kind::Location),
            "https://maps.example.com/embed",
            "https://www.example.com/",
        );
        assert_eq!(
            outcome,
            Outcome::Ask(Prompt {
                site: "example.com".into(),
                kind: Kind::Location,
                origin: Some("https://maps.example.com".into()),
            })
        );
        assert_eq!(
            page(&[], Kind::Location, "https://www.example.com/"),
            Outcome::Ask(prompt("example.com", Kind::Location))
        );
    }

    #[test]
    fn unasked_kinds_and_pages_out_of_sight_are_denied_without_asking() {
        let url = "https://example.com/";
        assert_eq!(decide(&[], true, None, url, url), Outcome::Deny);
        assert_eq!(
            decide(&[], false, Some(Kind::Location), url, url),
            Outcome::Deny
        );
        let list = [entry("example.com", Kind::Location, Decision::Allow)];
        assert_eq!(
            decide(&list, false, Some(Kind::Location), url, url),
            Outcome::Allow
        );
    }

    #[test]
    fn an_answer_settles_every_request_for_the_same_site_and_kind() {
        let mut queue = Queue::new();
        queue
            .push(prompt("example.com", Kind::Camera), "camera")
            .unwrap();
        queue
            .push(prompt("example.com", Kind::Microphone), "microphone")
            .unwrap();
        queue
            .push(prompt("example.com", Kind::Camera), "camera again")
            .unwrap();
        let (id, _) = queue.first().unwrap();
        let (answered, settled) = queue.answer(id).unwrap();
        assert_eq!(answered, prompt("example.com", Kind::Camera));
        assert_eq!(settled, ["camera", "camera again"]);
        assert_eq!(
            queue.first().map(|(_, prompt)| prompt.kind),
            Some(Kind::Microphone)
        );
        assert!(queue.answer(id).is_none());
    }

    #[test]
    fn a_full_queue_gives_the_request_back() {
        let mut queue = Queue::new();
        for n in 0..WAITING_MAX {
            queue.push(prompt("example.com", Kind::Camera), n).unwrap();
        }
        assert_eq!(
            queue.push(prompt("example.com", Kind::Camera), WAITING_MAX),
            Err(WAITING_MAX)
        );
    }

    #[test]
    fn saved_choices_keep_valid_entries_and_a_conflict_blocks() {
        let entries = serde_json::json!([
            { "site": "example.com", "kind": "location", "decision": "allow" },
            { "site": "example.com", "kind": "location", "decision": "block" },
            { "site": "example.com", "kind": "location", "decision": "allow" },
            { "site": "www.other.net", "kind": "camera", "decision": "allow" },
            { "site": "x.net#.example.com", "kind": "camera", "decision": "allow" },
            { "site": "other.net", "kind": "notifications", "decision": "allow" },
            { "site": "other.net", "kind": "camera", "decision": "maybe" },
            5,
            { "site": "other.net", "kind": "clipboardRead", "decision": "allow" },
        ]);
        let entries = serde_json::from_value(entries).unwrap();
        assert_eq!(
            from_settings(entries),
            [
                entry("example.com", Kind::Location, Decision::Block),
                entry("other.net", Kind::ClipboardRead, Decision::Allow),
            ]
        );
    }
}
