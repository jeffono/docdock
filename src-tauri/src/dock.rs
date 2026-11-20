//! Connects the state machine to the window: one thread polls the cursor, receives UI requests,
//! and animates sliding. A single thread avoids races between opening, closing, and animation.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::handle::{self, Presence};
use crate::motion::{self, Kind, Motion};
use crate::panel::{Action, CursorZone, Delays, Focus, Input, Panel, State};
use crate::platform::{self, Activation, BASE_DPI, Monitor, PanelWindow, Point, Rect, scale_dpi};

const POLL: Duration = Duration::from_millis(50);
/// Default width in logical pixels; on the monitor it becomes `width * dpi / 96`.
const PANEL_WIDTH: u32 = 460;
/// Narrowest panel; the widest is half the work area, never below this.
const MIN_WIDTH: u32 = 320;
/// Extra gap, in logical pixels, between the page and the inner edge: with the page's own 2 px inset it
/// leaves a 6 px strip of the UI, where the resize handle sits, along the whole panel.
const HANDLE_GAP: u32 = 4;
/// Two bars above the page, in logical pixels: twice `--size-bar` in `tokens.css`.
const WEB_TOP: u32 = 88;

/// Screen side the panel docks to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    #[default]
    Right,
}

/// UI requests to the panel thread.
#[derive(Debug, Clone, Copy)]
pub enum Request {
    Activate,
    SetPinned(bool),
    SetDialog(bool),
    /// Escape in the search bar, or the shortcut while already there; pinned, only releases focus.
    Close,
    /// The web page window was created and is still invisible.
    AttachWeb(isize),
    SetWebVisible(bool),
    SetWebOverlay(u32),
    /// Global shortcut.
    Shortcut,
    /// Side chosen in the tray.
    SetSide(Side),
    /// The tab the UI draws was clicked; only the resting open panel's tab counts.
    Tab,
    /// Logical width from the UI's resize handle, `None` for the default. `done` ends a drag and
    /// saves it; until then the panel stays open as if a dialog held it.
    Resize {
        width: Option<u32>,
        done: bool,
    },
}

/// Tells the UI whether the panel window or its page is foreground, and which edge is inner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct FocusReport {
    focused: bool,
    side: Side,
}

/// Asks the UI for the search bar. `toggle`: the panel window already had focus, so the UI closes
/// the panel instead if the bar already has it.
#[derive(Debug, Clone, Copy, Serialize)]
struct SearchRequest {
    toggle: bool,
}

/// Logical width within the limits of this monitor.
fn clamp_width(monitor: &Monitor, width: Option<u32>) -> u32 {
    let work = u32::try_from(monitor.work.right - monitor.work.left).unwrap_or(0);
    let max = (work * BASE_DPI / monitor.dpi.max(1) / 2).max(MIN_WIDTH);
    width.unwrap_or(PANEL_WIDTH).clamp(MIN_WIDTH, max)
}

/// Open panel rectangle, aligned with the work area edge on `side`.
fn panel_rect(monitor: &Monitor, side: Side, width: Option<u32>) -> Rect {
    let width = scale_dpi(monitor.dpi, clamp_width(monitor, width));
    match side {
        Side::Right => Rect {
            left: monitor.work.right - width,
            ..monitor.work
        },
        Side::Left => Rect {
            right: monitor.work.left + width,
            ..monitor.work
        },
    }
}

/// X of the closed panel: just beyond the monitor edge on `side`.
fn parked_x(monitor: &Monitor, side: Side, panel: Rect) -> i32 {
    match side {
        Side::Right => monitor.bounds.right,
        Side::Left => monitor.bounds.left - (panel.right - panel.left),
    }
}

/// Web page rectangle: the panel minus the top bar and the handle gap on the inner edge.
fn web_rect(monitor: &Monitor, panel: Rect, side: Side) -> Rect {
    let gap = scale_dpi(monitor.dpi, HANDLE_GAP);
    let (left, right) = match side {
        Side::Right => ((panel.left + gap).min(panel.right - 1), panel.right),
        Side::Left => (panel.left, (panel.right - gap).max(panel.left + 1)),
    };
    Rect {
        top: (panel.top + scale_dpi(monitor.dpi, WEB_TOP)).min(panel.bottom - 1),
        left,
        right,
        ..panel
    }
}

/// Frames the page over the panel, retaining its offset for batched placement.
fn frame_page(web: &mut PanelWindow, monitor: &Monitor, panel: Rect, side: Side) {
    let rect = web_rect(monitor, panel, side);
    web.set_offset(rect.left - panel.left);
    web.set_rect(rect, monitor.bounds);
}

fn place_page(web: &mut PanelWindow, monitor: &Monitor, panel: Rect, side: Side, x: i32) {
    frame_page(web, monitor, panel, side);
    platform::place_windows(&[web], x);
}

/// Only the outer edge of the outermost monitor on `side` holds the cursor; elsewhere it would cross
/// into a neighboring monitor. It also puts the parked position outside every screen. On a tie, such
/// as stacked monitors, prefer the primary monitor, whose origin is (0, 0).
fn edge_monitor(monitors: Vec<Monitor>, side: Side) -> Option<Monitor> {
    monitors.into_iter().max_by_key(|monitor| {
        let primary = monitor.bounds.left == 0 && monitor.bounds.top == 0;
        let outer = match side {
            Side::Right => monitor.bounds.right,
            Side::Left => -monitor.bounds.left,
        };
        (outer, primary)
    })
}

/// The new panel monitor if a layout, resolution, DPI, or taskbar change changed the current one.
/// If no monitors are available, as during a transition, retain the current one.
fn monitor_change(current: &Monitor, monitors: Vec<Monitor>, side: Side) -> Option<Monitor> {
    edge_monitor(monitors, side).filter(|monitor| monitor != current)
}

struct Slide {
    from: i32,
    to: i32,
    kind: Kind,
    motion: Motion,
    started: Instant,
    last_frame: Instant,
    frames: u32,
    longest_frame: Duration,
}

impl Slide {
    fn new(from: i32, to: i32, kind: Kind) -> Self {
        let now = Instant::now();
        Self {
            from,
            to,
            kind,
            motion: motion::current(kind),
            started: now,
            last_frame: now,
            frames: 0,
            longest_frame: Duration::ZERO,
        }
    }

    /// A slide in step with `leader`: same motion and start, so both land on the same frame.
    fn along(from: i32, to: i32, leader: &Slide) -> Self {
        Self {
            motion: leader.motion,
            started: leader.started,
            ..Self::new(from, to, leader.kind)
        }
    }

    /// Advances one frame and returns the position and whether the slide is done.
    fn advance(&mut self, now: Instant) -> (i32, bool) {
        self.frames += 1;
        self.longest_frame = self.longest_frame.max(now - self.last_frame);
        self.last_frame = now;
        let elapsed = now - self.started;
        let position = self.motion.position(self.from, self.to, elapsed);
        (position, self.motion.done(elapsed))
    }

    fn report(&self, label: &str) {
        eprintln!(
            "[panel] {label}: {:.1} ms, {} frames, longest interval {:.1} ms",
            self.started.elapsed().as_secs_f64() * 1000.0,
            self.frames,
            self.longest_frame.as_secs_f64() * 1000.0,
        );
    }
}

struct Dock {
    app: AppHandle,
    /// The panel's window, wider than the panel by the tab its WebView draws on the inner side.
    window: PanelWindow,
    /// The panel's window lets every click through, as it does parked.
    click_through: bool,
    /// Web page window, if present.
    web: Option<PanelWindow>,
    /// The edge handle: the tab beside the panel, which the panel's WebView draws.
    handle_presence: Presence,
    /// How far the handle is pushed out beyond its place, toward the screen edge, and the panel's
    /// windows with it: it slides in from the edge when it appears and back out when it hides, so
    /// a hidden handle takes the parked panel's tab beyond the screen edge.
    handle_shift: i32,
    handle_slide: Option<Slide>,
    reveal: handle::Reveal,
    /// The handle values in use, the tab's shape among them: read on each cursor poll, so one poll's
    /// rules and the windows' geometry agree.
    tuning: handle::Tuning,
    /// The last polled cursor, to measure its speed toward the edge.
    last_cursor: Option<(Point, Instant)>,
    /// The cursor heads for the edge fast enough to open the panel without waiting.
    rushing: bool,
    bar_use: handle::BarUse,
    web_visible: bool,
    /// WebView2 was asked to trim memory because the panel is parked.
    memory_low: bool,
    overlay_height: u32,
    side: Side,
    /// Logical width chosen by the user, `None` for the default.
    width: Option<u32>,
    /// A resize drag is in progress: the cursor may leave the panel without closing it.
    resizing: bool,
    monitor: Monitor,
    rect: Rect,
    parked_x: i32,
    x: i32,
    panel: Panel,
    dialog: bool,
    /// Last focus sent to the UI, to send only changes.
    reported: Option<FocusReport>,
    /// Last foreground app outside the panel, to restore focus when closing.
    previous_focus: Option<isize>,
    /// This Escape press has already been seen or handled.
    escape_spent: bool,
    slide: Option<Slide>,
}

impl Dock {
    fn is_open(&self) -> bool {
        matches!(
            self.panel.state,
            State::Open { .. } | State::Summoned { .. } | State::Pinned
        )
    }

    /// Takes the latest handle values. A new tab shape resizes the window's strip beside the panel
    /// and reshapes its region in place, and the UI redraws the tab from the shape it is sent.
    fn retune(&mut self) {
        let tuning = handle_tuning();
        let reshaped = tuning.tab != self.tuning.tab;
        self.tuning = tuning;
        if reshaped {
            self.place(self.monitor);
            let _ = self.app.emit_to("main", "panel-tab-shape", tuning.tab);
        }
    }

    /// Moves windows to the monitor and side without animation. Any ongoing slide used the old ones.
    fn relocate(&mut self, monitor: Monitor) {
        eprintln!("[panel] placed on {:?}: {monitor:?}", self.side);
        self.place(monitor);
    }

    fn place(&mut self, monitor: Monitor) {
        self.monitor = monitor;
        self.rect = panel_rect(&monitor, self.side, self.width);
        self.parked_x = parked_x(&monitor, self.side, self.rect);
        self.slide = None;
        self.x = if self.is_open() {
            self.rect.left
        } else {
            self.parked_x
        };
        // A close cut short here still parks the panel, which lets clicks through as after a slide.
        self.set_click_through(self.x == self.parked_x);
        // The handle settles where it was going.
        self.handle_slide = None;
        self.handle_shift = if self.handle_presence == Presence::Hidden {
            self.away()
        } else {
            0
        };
        self.frame_window();
        let x = self.window_x();
        let mut page = None;
        if self.web_visible
            && let Some(web) = &mut self.web
        {
            web.overlay(monitor.dpi, self.overlay_height, x);
            frame_page(web, &monitor, self.rect, self.side);
            page = Some(&*web);
        }
        // Resizing moves the panel, its tab and its page: together, so none trails the others.
        let mut windows = vec![&self.window];
        windows.extend(page);
        platform::place_windows(&windows, x);
    }

    /// Sizes the panel's window to the panel and the strip beside it where the tab shows, and
    /// shapes it to their outline.
    fn frame_window(&mut self) {
        let tab = self.tuning.tab;
        let framed = handle::framed_rect(&self.monitor, self.rect, self.side, tab);
        self.window.set_offset(framed.left - self.rect.left);
        self.window.set_outline(handle::Outline::new(
            &self.monitor,
            self.rect,
            self.side,
            tab,
        ));
        self.window.set_rect(framed, self.monitor.bounds);
    }

    /// Where the panel's windows are: the panel's `x`, pushed out by the handle's shift.
    fn window_x(&self) -> i32 {
        match self.side {
            Side::Right => self.x + self.handle_shift,
            Side::Left => self.x - self.handle_shift,
        }
    }

    /// Out of sight beyond the screen edge, whether the panel is parked or not: the tab's depth.
    fn away(&self) -> i32 {
        handle::Shape::new(self.tuning.tab, self.monitor.dpi).depth
    }

    fn set_click_through(&mut self, through: bool) {
        if self.click_through != through {
            self.click_through = through;
            self.window.set_click_through(through);
        }
    }

    fn update_handle(&mut self) {
        let near = platform::cursor_pos()
            .is_some_and(|cursor| handle::near(cursor, &self.monitor, self.side, self.tuning));
        let revealed = self.reveal.step(near, self.rushing);
        let presence = handle::presence(self.is_open(), self.slide.is_some(), revealed);
        self.handle_presence = presence;
        let target = if presence == Presence::Hidden {
            self.away()
        } else {
            0
        };
        let heading = self
            .handle_slide
            .as_ref()
            .map_or(self.handle_shift, |slide| slide.to);
        if heading != target {
            let (kind, panel) = if target == 0 {
                (Kind::HandleIn, Kind::Open)
            } else {
                (Kind::HandleOut, Kind::Close)
            };
            // Going the panel's way, it moves in step with it: a hidden handle comes in with the
            // opening panel instead of sliding in on its own, and leaves with the closing one.
            self.handle_slide = Some(match &self.slide {
                Some(slide) if slide.kind == panel => {
                    Slide::along(self.handle_shift, target, slide)
                }
                _ => Slide::new(self.handle_shift, target, kind),
            });
        }
    }

    fn handle(&mut self, request: Request) {
        let now = Instant::now();
        let action = match request {
            Request::Activate => {
                if !self.window.is_foreground() {
                    self.window.activate();
                }
                Action::None
            }
            Request::SetDialog(dialog) => {
                self.dialog = dialog;
                Action::None
            }
            Request::Close => self.panel.step(Input::Close, now),
            Request::AttachWeb(raw) => {
                // A page attached while parked gets the low target on the next pass, with `main`.
                self.memory_low = false;
                let x = self.window_x();
                let mut web = PanelWindow::prepare(raw, Activation::OnClick);
                frame_page(&mut web, &self.monitor, self.rect, self.side);
                web.overlay(self.monitor.dpi, self.overlay_height, x);
                platform::place_windows(&[&web], x);
                web.set_enabled(self.is_open());
                self.web = Some(web);
                self.web_visible = true;
                let _ = self.app.emit_to("main", "web-visible", true);
                Action::None
            }
            Request::SetWebVisible(visible) => {
                let x = self.window_x();
                if let Some(mut web) = self.web.filter(PanelWindow::exists) {
                    if !visible {
                        if web.is_foreground() {
                            self.window.activate();
                        }
                        web.hide();
                    } else {
                        web.overlay(self.monitor.dpi, self.overlay_height, x);
                        place_page(&mut web, &self.monitor, self.rect, self.side, x);
                        web.set_enabled(self.is_open());
                    }
                    self.web = Some(web);
                }
                self.web_visible = visible;
                let _ = self.app.emit_to("main", "web-visible", visible);
                Action::None
            }
            Request::SetWebOverlay(height) => {
                self.overlay_height = height;
                let x = self.window_x();
                if let Some(web) = &mut self.web {
                    web.overlay(self.monitor.dpi, height, x);
                }
                Action::None
            }
            Request::SetPinned(true) => self.panel.step(Input::Pin, now),
            Request::SetPinned(false) => self.panel.step(Input::Unpin, now),
            Request::Shortcut => {
                // Read before activating: only a press made inside the panel can close it.
                let toggle = self.window.is_foreground();
                let dialog = self.dialog;
                let action = self.panel.step(Input::Shortcut { dialog }, now);
                self.apply(action);
                if action == Action::Summon {
                    let _ = self
                        .app
                        .emit_to("main", "panel-search", SearchRequest { toggle });
                }
                Action::None
            }
            Request::SetSide(side) => {
                self.side = side;
                // Without monitors, as during a transition, the current one still flips side.
                let monitor = edge_monitor(platform::monitors(), side).unwrap_or(self.monitor);
                self.relocate(monitor);
                Action::None
            }
            // A tab sliding under the cursor, or the closed panel's, takes no click.
            Request::Tab if self.handle_presence != Presence::Tab => Action::None,
            Request::Tab => {
                let pinned = self.panel.state == State::Pinned;
                let action = self.panel.step(
                    Input::Tab {
                        dialog: self.dialog,
                    },
                    now,
                );
                // The UI's pin button follows: the panel closed unpinned.
                if pinned && action == Action::Hide {
                    let _ = self.app.emit_to("main", "panel-pinned", false);
                }
                action
            }
            Request::Resize { width, done } => {
                self.width = width.map(|width| clamp_width(&self.monitor, Some(width)));
                self.resizing = !done;
                self.place(self.monitor);
                if done && let Err(error) = crate::notes::set_width(&self.app, self.width) {
                    eprintln!("[panel] failed to save width: {error:?}");
                }
                Action::None
            }
        };
        self.apply(action);
        self.report_shown();
        self.update_handle();
    }

    fn poll(&mut self) {
        let Some(cursor) = platform::cursor_pos() else {
            return;
        };
        if let Some(monitor) = monitor_change(&self.monitor, platform::monitors(), self.side) {
            self.relocate(monitor);
        }
        self.retune();
        // A drag whose end never reached Rust (lost capture, rejected request) must not hold the
        // panel open forever: the released button ends it with the last width.
        if self.resizing && !platform::primary_button_down() {
            self.handle(Request::Resize {
                width: self.width,
                done: true,
            });
        }
        self.track_focus();
        let tuning = self.tuning;
        let now = Instant::now();
        self.rushing = self.last_cursor.is_some_and(|(from, then)| {
            let elapsed = now - then;
            handle::rushing(from, cursor, elapsed, &self.monitor, self.side, tuning.rush)
        });
        self.last_cursor = Some((cursor, now));
        let zone = handle::cursor_zone(cursor, &self.monitor, self.rect, self.side, tuning);
        let zone = self.bar_use.zone(
            zone,
            handle::over_bars(cursor, &self.monitor, self.rect),
            platform::primary_button_down(),
            platform::foreground_window(),
        );
        // A user typing on the page can move the mouse away without losing the panel. The page does
        // not talk to Rust, so Escape is read from the keyboard; over the page it remains the page's.
        // Once focus is restored, the panel closes as with any other focus loss.
        let focus = self.focus();
        let page_focused = focus == Focus::Page;
        let escape = self.escape_tap();
        if page_focused && zone == CursorZone::Outside && escape {
            platform::restore_focus(self.previous_focus);
        }
        // A site's permission request holds the panel open, as a dialog does, until answered.
        let tick = Input::Tick {
            zone,
            dialog: self.dialog || self.resizing || crate::permissions::asking(),
            focus,
            rush: zone == CursorZone::Edge && self.rushing,
        };
        let action = self.panel.step(tick, now);
        self.apply(action);
        self.report_focus(focus);
        self.report_shown();
        self.update_handle();
    }

    /// Permission requests are asked only while the page is on screen.
    fn report_shown(&self) {
        let shown = self.is_open() && self.web_visible && self.web.is_some();
        crate::permissions::set_shown(&self.app, shown);
    }

    /// The accent band follows this. Polling sees the page too, which the UI's own focus events
    /// cannot, and null focus during transitions keeps the last report instead of flickering.
    fn report_focus(&mut self, focus: Focus) {
        if focus == Focus::Unknown {
            return;
        }
        let report = FocusReport {
            focused: matches!(focus, Focus::Panel | Focus::Page),
            side: self.side,
        };
        if self.reported != Some(report) {
            self.reported = Some(report);
            // A recording the UI abandons must not leave the activation shortcut released.
            if !report.focused {
                crate::shortcut::end_recording(&self.app);
            }
            let _ = self.app.emit_to("main", "panel-focus", report);
        }
    }

    fn focus(&self) -> Focus {
        if self.window.is_foreground() {
            Focus::Panel
        } else if self.web_visible && self.web.is_some_and(|web| web.is_foreground()) {
            Focus::Page
        } else if platform::foreground_window().is_some() {
            Focus::Elsewhere
        } else {
            Focus::Unknown
        }
    }

    /// A new Escape press. Reading the key on each poll avoids missing presses and counts each only
    /// once, even when held down.
    fn escape_tap(&mut self) -> bool {
        let pressed = platform::escape_pressed();
        let tap = pressed && !self.escape_spent;
        self.escape_spent = pressed;
        tap
    }

    /// Panel windows. The page window disappears from the list if destroyed, for example by Alt+F4.
    fn windows(&mut self) -> Vec<PanelWindow> {
        if self.web.is_some_and(|web| !web.exists()) {
            self.web = None;
            self.web_visible = false;
            let _ = self.app.emit_to("main", "web-state", Option::<()>::None);
            let _ = self.app.emit_to("main", "web-visible", false);
        }
        std::iter::once(self.window)
            .chain(self.web.filter(|_| self.web_visible))
            .collect()
    }

    fn is_foreground(&mut self) -> bool {
        self.windows().iter().any(PanelWindow::is_foreground)
    }

    /// The page receives focus directly from Windows, bypassing the UI, so the panel tracks the
    /// foreground window instead of saving the previous window only when requesting activation.
    fn track_focus(&mut self) {
        // The native picker belongs to the panel and disappears on close; focus must not return to it.
        if !self.dialog && !self.is_foreground() {
            self.previous_focus = platform::foreground_window().or(self.previous_focus);
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        for window in self.windows() {
            window.set_enabled(enabled);
        }
    }

    fn show(&mut self) {
        self.set_click_through(false);
        self.set_enabled(true);
        self.window.raise();
        crate::tray::check_glyph(&self.app);
        // Summoning an open panel only gives it focus.
        if self.x != self.rect.left || self.slide.is_some() {
            self.slide = Some(Slide::new(self.x, self.rect.left, Kind::Open));
        }
    }

    /// Trims memory of both webviews while parked and still, including at startup and after a slide
    /// cut short by a side or monitor change, and restores it as soon as the panel starts to open.
    fn update_memory(&mut self) {
        let low = self.x == self.parked_x && self.slide.is_none();
        if self.memory_low == low {
            return;
        }
        self.memory_low = low;
        for label in ["main", "web"] {
            if let Some(window) = self.app.get_webview_window(label) {
                platform::set_memory_low(&window, low);
            }
        }
    }

    /// Returns focus to the previous app. If the user already clicked another app, leave its focus alone.
    fn release_focus(&mut self) {
        if self.is_foreground() {
            platform::restore_focus(self.previous_focus);
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Show => self.show(),
            Action::Summon => {
                // The poll may be up to 50 ms old, and after activation it would be too late.
                self.track_focus();
                // Activation requires an enabled window. Confirming here distinguishes failure from
                // focus lost immediately afterward, before the next poll.
                self.show();
                if self.window.activate() {
                    self.panel.step(Input::Activated, Instant::now());
                }
            }
            Action::Hide => {
                self.reveal.hold();
                self.release_focus();
                self.set_enabled(false);
                self.slide = Some(Slide::new(self.x, self.parked_x, Kind::Close));
            }
            // The panel is foreground, so Windows lets it hand focus over.
            Action::Release => self.release_focus(),
        }
    }

    /// Draws a frame if a slide is in progress.
    fn animate(&mut self) {
        if self.slide.is_none() && self.handle_slide.is_none() {
            return;
        }
        let now = Instant::now();
        if let Some(slide) = &mut self.handle_slide {
            let (shift, done) = slide.advance(now);
            self.handle_shift = shift;
            if done {
                self.handle_slide = None;
            }
        }
        // The panel and the handle's shift move the same windows, in one batch.
        let landed = self.slide.as_mut().and_then(|slide| {
            let (x, done) = slide.advance(now);
            self.x = x;
            done.then_some(x)
        });
        let windows = self.windows();
        platform::slide_windows(&windows, self.window_x());
        let Some(x) = landed else {
            return;
        };
        let parked = x == self.parked_x;
        if let Some(slide) = self.slide.take() {
            slide.report(if parked { "close" } else { "open" });
        }
        if parked {
            self.set_click_through(true);
            // A hide ends the session: a deleted note can no longer be undone. The UI hears of it
            // only through `panel-hidden`: `visibilitychange` does not fire for a window moved
            // off-screen. Once it saved the editor it leaves, it sends the deleted notes' images.
            crate::notes::forget_deleted();
            crate::notes::purge_trash(&self.app);
            crate::shortcut::end_recording(&self.app);
            let _ = self.app.emit_to("main", "panel-hidden", ());
        }
        self.update_handle();
    }

    fn run(mut self, requests: Receiver<Request>) {
        let mut next_poll = Instant::now();
        loop {
            if self.slide.is_some() || self.handle_slide.is_some() {
                platform::wait_frame();
                while let Ok(request) = requests.try_recv() {
                    self.handle(request);
                }
            } else {
                match requests.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
                    Ok(request) => self.handle(request),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            if Instant::now() >= next_poll {
                self.poll();
                next_poll = Instant::now() + POLL;
            }
            self.animate();
            self.update_memory();
        }
    }
}

/// Prepares the window off-screen, its tab hidden and letting clicks through, and starts the panel
/// thread.
pub fn start(
    app: AppHandle,
    raw_window: isize,
    requests: Receiver<Request>,
    side: Side,
    width: Option<u32>,
) {
    let monitor = edge_monitor(platform::monitors(), side).expect("no monitor available");
    let rect = panel_rect(&monitor, side, width);
    let parked_x = parked_x(&monitor, side, rect);
    let window = PanelWindow::prepare(raw_window, Activation::OnRequest);
    eprintln!("[panel] {side:?} of monitor {monitor:?}, panel {rect:?}");
    let mut dock = Dock {
        app,
        window,
        click_through: false,
        web: None,
        handle_presence: Presence::Hidden,
        handle_shift: 0,
        handle_slide: None,
        reveal: handle::Reveal::default(),
        tuning: handle_tuning(),
        last_cursor: None,
        rushing: false,
        bar_use: handle::BarUse::default(),
        web_visible: false,
        memory_low: false,
        overlay_height: 0,
        side,
        width,
        resizing: false,
        monitor,
        rect,
        parked_x,
        x: parked_x,
        panel: Panel::new(Delays::default()),
        dialog: false,
        reported: None,
        previous_focus: None,
        escape_spent: false,
        slide: None,
    };
    dock.place(monitor);
    std::thread::spawn(move || dock.run(requests));
}

/// The edge handle's constants in release builds, live values from the development tweak window
/// otherwise.
pub fn handle_tuning() -> handle::Tuning {
    #[cfg(debug_assertions)]
    return crate::dev::handle_tuning();
    #[cfg(not(debug_assertions))]
    handle::TUNING
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(left: i32, right: i32) -> Monitor {
        let bounds = Rect {
            left,
            top: 0,
            right,
            bottom: 2160,
        };
        Monitor {
            bounds,
            work: bounds,
            dpi: 120,
        }
    }

    #[test]
    fn panel_stays_on_outermost_monitor_of_its_side() {
        let primary = monitor(0, 3840);
        let pair = vec![primary, monitor(3840, 7680)];
        let chosen = edge_monitor(pair.clone(), Side::Right).unwrap();
        assert_eq!(chosen.bounds.right, 7680);
        assert_eq!(edge_monitor(pair, Side::Left).unwrap(), primary);
        let pair = vec![monitor(-3840, 0), primary];
        assert_eq!(edge_monitor(pair.clone(), Side::Right).unwrap(), primary);
        let chosen = edge_monitor(pair, Side::Left).unwrap();
        assert_eq!(chosen.bounds.left, -3840);
        // Stacked monitors tie on both sides: the primary wins in any order.
        let below = Monitor {
            bounds: Rect {
                top: 2160,
                bottom: 4320,
                ..primary.bounds
            },
            ..primary
        };
        for side in [Side::Right, Side::Left] {
            assert_eq!(edge_monitor(vec![primary, below], side).unwrap(), primary);
            assert_eq!(edge_monitor(vec![below, primary], side).unwrap(), primary);
        }
    }

    #[test]
    fn swapping_monitors_while_app_is_open_moves_panel() {
        let current = monitor(3840, 7680);
        let reversed = vec![monitor(-3840, 0), monitor(0, 3840)];
        let changed = monitor_change(&current, reversed, Side::Right).unwrap();
        assert_eq!(changed.bounds.right, 3840);
        let pair = vec![monitor(0, 3840), current];
        assert!(monitor_change(&current, pair.clone(), Side::Right).is_none());
        assert!(monitor_change(&current, Vec::new(), Side::Right).is_none());
        let changed = monitor_change(&current, pair, Side::Left).unwrap();
        assert_eq!(changed.bounds.left, 0);
    }

    #[test]
    fn left_side_mirrors_open_and_parked_positions() {
        let mut screen = monitor(-3840, 0);
        screen.work = Rect {
            left: -3800,
            top: 47,
            right: -40,
            bottom: 2160,
        };
        let rect = panel_rect(&screen, Side::Right, None);
        assert_eq!((rect.left, rect.right, rect.top), (-615, -40, 47));
        assert_eq!(parked_x(&screen, Side::Right, rect), 0);
        let rect = panel_rect(&screen, Side::Left, None);
        assert_eq!((rect.left, rect.right, rect.top), (-3800, -3225, 47));
        // Parked entirely beyond the left edge, touching it.
        assert_eq!(parked_x(&screen, Side::Left, rect), -3840 - 575);
    }

    // Catches a width saved on a large monitor covering most of a smaller one, or a sliver panel.
    #[test]
    fn width_stays_between_minimum_and_half_the_work_area() {
        let mut small = monitor(0, 1366);
        small.dpi = 96;
        assert_eq!(clamp_width(&small, Some(1200)), 683);
        assert_eq!(clamp_width(&small, Some(100)), MIN_WIDTH);
        assert_eq!(clamp_width(&small, None), PANEL_WIDTH);
        // A monitor narrower than twice the minimum still gets the minimum.
        let mut tiny = monitor(0, 500);
        tiny.dpi = 96;
        assert_eq!(clamp_width(&tiny, Some(400)), MIN_WIDTH);
    }

    // Catches the page covering the resize handle on the inner edge.
    #[test]
    fn the_page_leaves_the_inner_edge_to_the_handle() {
        let screen = monitor(0, 3840);
        let panel = panel_rect(&screen, Side::Right, None);
        let page = web_rect(&screen, panel, Side::Right);
        assert_eq!((page.left - panel.left, page.right), (5, panel.right));
        let panel = panel_rect(&screen, Side::Left, None);
        let page = web_rect(&screen, panel, Side::Left);
        assert_eq!((page.left, panel.right - page.right), (panel.left, 5));
    }
}
