//! Connects the state machine to the window: one thread polls the cursor, receives UI requests,
//! and animates sliding. A single thread avoids races between opening, closing, and animation.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::panel::{Action, CursorZone, Delays, Focus, Input, Panel, State};
use crate::platform::{self, Activation, Monitor, PanelWindow, Point, Rect};

const POLL: Duration = Duration::from_millis(50);
const SLIDE: Duration = Duration::from_millis(150);
/// Default width in logical pixels; on the monitor it becomes `width * dpi / 96`.
const PANEL_WIDTH: u32 = 460;
/// Narrowest panel; the widest is half the work area, never below this.
const MIN_WIDTH: u32 = 320;
/// Extra gap, in logical pixels, between the page and the inner edge: with the page's own 2 px inset it
/// leaves a 6 px strip of the UI, where the resize handle sits, along the whole panel.
const HANDLE_GAP: u32 = 4;
const BASE_DPI: u32 = 96;
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
    let width = (clamp_width(monitor, width) * monitor.dpi / BASE_DPI) as i32;
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
    let gap = (HANDLE_GAP * monitor.dpi / BASE_DPI) as i32;
    let (left, right) = match side {
        Side::Right => ((panel.left + gap).min(panel.right - 1), panel.right),
        Side::Left => (panel.left, (panel.right - gap).max(panel.left + 1)),
    };
    Rect {
        top: (panel.top + (WEB_TOP * monitor.dpi / BASE_DPI) as i32).min(panel.bottom - 1),
        left,
        right,
        ..panel
    }
}

/// Places the page window over the open panel `panel`, sliding with it at `x`.
fn place_page(web: &mut PanelWindow, monitor: &Monitor, panel: Rect, side: Side, x: i32) {
    let rect = web_rect(monitor, panel, side);
    web.set_offset(rect.left - panel.left);
    web.place(rect, monitor.bounds, x);
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

/// Translates the cursor position. Points outside the monitor never open or hold the panel.
fn cursor_zone(cursor: Point, monitor: Rect, panel: Rect, side: Side) -> CursorZone {
    let edge = match side {
        Side::Right => monitor.right - 1,
        Side::Left => monitor.left,
    };
    if !monitor.contains(cursor) {
        CursorZone::Outside
    } else if cursor.x == edge {
        CursorZone::Edge
    } else if panel.contains(cursor) {
        CursorZone::Inside
    } else {
        CursorZone::Outside
    }
}

/// Slide position after `elapsed`, easing out at the end. It depends on elapsed time, not frame
/// count: a late frame jumps ahead instead of extending the animation.
fn slide_x(from: i32, to: i32, elapsed: Duration) -> i32 {
    let t = (elapsed.as_secs_f64() / SLIDE.as_secs_f64()).min(1.0);
    let eased = 1.0 - (1.0 - t).powi(3);
    from + (f64::from(to - from) * eased).round() as i32
}

struct Slide {
    from: i32,
    to: i32,
    started: Instant,
    last_frame: Instant,
    frames: u32,
    longest_frame: Duration,
}

impl Slide {
    fn new(from: i32, to: i32) -> Self {
        let now = Instant::now();
        Self {
            from,
            to,
            started: now,
            last_frame: now,
            frames: 0,
            longest_frame: Duration::ZERO,
        }
    }

    /// Advances one frame and returns the position and whether the slide is done.
    fn advance(&mut self, now: Instant) -> (i32, bool) {
        self.frames += 1;
        self.longest_frame = self.longest_frame.max(now - self.last_frame);
        self.last_frame = now;
        let elapsed = now - self.started;
        (slide_x(self.from, self.to, elapsed), elapsed >= SLIDE)
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
    window: PanelWindow,
    /// Web page window, if present.
    web: Option<PanelWindow>,
    web_visible: bool,
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
        self.window.place(self.rect, monitor.bounds, self.x);
        if self.web_visible
            && let Some(web) = &mut self.web
        {
            web.overlay(monitor.dpi, self.overlay_height, self.x);
            place_page(web, &monitor, self.rect, self.side, self.x);
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
                let rect = web_rect(&self.monitor, self.rect, self.side);
                let bounds = self.monitor.bounds;
                let mut web = PanelWindow::prepare(raw, rect, bounds, self.x, Activation::OnClick);
                web.overlay(self.monitor.dpi, self.overlay_height, self.x);
                place_page(&mut web, &self.monitor, self.rect, self.side, self.x);
                web.set_enabled(self.is_open());
                self.web = Some(web);
                self.web_visible = true;
                let _ = self.app.emit_to("main", "web-visible", true);
                Action::None
            }
            Request::SetWebVisible(visible) => {
                if let Some(mut web) = self.web.filter(PanelWindow::exists) {
                    if !visible {
                        if web.is_foreground() {
                            self.window.activate();
                        }
                        web.hide();
                    } else {
                        web.overlay(self.monitor.dpi, self.overlay_height, self.x);
                        place_page(&mut web, &self.monitor, self.rect, self.side, self.x);
                        web.set_enabled(self.is_open());
                    }
                    self.web = Some(web);
                    self.web_visible = visible;
                    let _ = self.app.emit_to("main", "web-visible", visible);
                } else {
                    self.web_visible = visible;
                    let _ = self.app.emit_to("main", "web-visible", visible);
                }
                Action::None
            }
            Request::SetWebOverlay(height) => {
                self.overlay_height = height;
                if let Some(web) = &mut self.web {
                    web.overlay(self.monitor.dpi, height, self.x);
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
    }

    fn poll(&mut self) {
        let Some(cursor) = platform::cursor_pos() else {
            return;
        };
        if let Some(monitor) = monitor_change(&self.monitor, platform::monitors(), self.side) {
            self.relocate(monitor);
        }
        // A drag whose end never reached Rust (lost capture, rejected request) must not hold the
        // panel open forever: the released button ends it with the last width.
        if self.resizing && !platform::primary_button_down() {
            self.handle(Request::Resize {
                width: self.width,
                done: true,
            });
        }
        self.track_focus();
        let zone = cursor_zone(cursor, self.monitor.bounds, self.rect, self.side);
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
        };
        let action = self.panel.step(tick, Instant::now());
        self.apply(action);
        self.report_focus(focus);
        self.report_shown();
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
        self.set_enabled(true);
        self.window.raise();
        // Summoning an open panel only gives it focus.
        if self.x != self.rect.left || self.slide.is_some() {
            self.slide = Some(Slide::new(self.x, self.rect.left));
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
                self.release_focus();
                self.set_enabled(false);
                self.slide = Some(Slide::new(self.x, self.parked_x));
            }
            // The panel is foreground, so Windows lets it hand focus over.
            Action::Release => self.release_focus(),
        }
    }

    /// Draws a frame if a slide is in progress.
    fn animate(&mut self) {
        let windows = self.windows();
        let Some(slide) = &mut self.slide else {
            return;
        };
        let (x, done) = slide.advance(Instant::now());
        platform::slide_windows(&windows, x);
        self.x = x;
        if done {
            let label = if x == self.parked_x { "close" } else { "open" };
            slide.report(label);
            self.slide = None;
            if x == self.parked_x {
                // The UI folds its expanded card back to a preview while nobody sees it, and a
                // deleted note can no longer be undone.
                crate::notes::forget_deleted();
                crate::notes::purge_trash(&self.app);
                crate::shortcut::end_recording(&self.app);
                let _ = self.app.emit_to("main", "panel-hidden", ());
            }
        }
    }

    fn run(mut self, requests: Receiver<Request>) {
        let mut next_poll = Instant::now();
        loop {
            if self.slide.is_some() {
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
        }
    }
}

/// Prepares the window off-screen and starts the panel thread.
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
    let bounds = monitor.bounds;
    let window = PanelWindow::prepare(raw_window, rect, bounds, parked_x, Activation::OnRequest);
    eprintln!("[panel] {side:?} of monitor {monitor:?}, panel {rect:?}");
    let dock = Dock {
        app,
        window,
        web: None,
        web_visible: false,
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
    std::thread::spawn(move || dock.run(requests));
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

    const PRIMARY: Rect = Rect {
        left: 0,
        top: 0,
        right: 3840,
        bottom: 2160,
    };
    const PANEL: Rect = Rect {
        left: 3315,
        top: 47,
        right: 3840,
        bottom: 2100,
    };

    fn zone(x: i32, y: i32, side: Side) -> CursorZone {
        let panel = match side {
            Side::Right => PANEL,
            Side::Left => Rect {
                left: 0,
                right: 525,
                ..PANEL
            },
        };
        cursor_zone(Point { x, y }, PRIMARY, panel, side)
    }

    #[test]
    fn panel_stays_on_outermost_monitor_of_its_side() {
        let principal = monitor(0, 3840);
        let pair = vec![principal, monitor(3840, 7680)];
        let chosen = edge_monitor(pair.clone(), Side::Right).unwrap();
        assert_eq!(chosen.bounds.right, 7680);
        assert_eq!(edge_monitor(pair, Side::Left).unwrap(), principal);
        let pair = vec![monitor(-3840, 0), principal];
        assert_eq!(edge_monitor(pair.clone(), Side::Right).unwrap(), principal);
        let chosen = edge_monitor(pair, Side::Left).unwrap();
        assert_eq!(chosen.bounds.left, -3840);
        // Stacked monitors tie on both sides: the primary wins in any order.
        let below = Monitor {
            bounds: Rect {
                top: 2160,
                bottom: 4320,
                ..PRIMARY
            },
            ..principal
        };
        for side in [Side::Right, Side::Left] {
            assert_eq!(
                edge_monitor(vec![principal, below], side).unwrap(),
                principal
            );
            assert_eq!(
                edge_monitor(vec![below, principal], side).unwrap(),
                principal
            );
        }
    }

    #[test]
    fn swapping_monitors_while_app_is_open_moves_panel() {
        let atual = monitor(3840, 7680);
        let invertido = vec![monitor(-3840, 0), monitor(0, 3840)];
        let novo = monitor_change(&atual, invertido, Side::Right).unwrap();
        assert_eq!(novo.bounds.right, 3840);
        let pair = vec![monitor(0, 3840), atual];
        assert!(monitor_change(&atual, pair.clone(), Side::Right).is_none());
        assert!(monitor_change(&atual, Vec::new(), Side::Right).is_none());
        let novo = monitor_change(&atual, pair, Side::Left).unwrap();
        assert_eq!(novo.bounds.left, 0);
    }

    #[test]
    fn only_outer_pixel_column_of_monitor_is_edge() {
        assert_eq!(zone(3839, 0, Side::Right), CursorZone::Edge);
        assert_eq!(zone(3839, 2159, Side::Right), CursorZone::Edge);
        assert_eq!(zone(3838, 0, Side::Right), CursorZone::Outside);
        assert_eq!(zone(0, 0, Side::Left), CursorZone::Edge);
        assert_eq!(zone(0, 2159, Side::Left), CursorZone::Edge);
        assert_eq!(zone(1, 0, Side::Left), CursorZone::Outside);
        assert_eq!(zone(3839, 0, Side::Left), CursorZone::Outside);
        // Points on other monitors are ignored.
        assert_eq!(zone(-1, 500, Side::Right), CursorZone::Outside);
        assert_eq!(zone(-1, 500, Side::Left), CursorZone::Outside);
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

    #[test]
    fn slide_ends_exactly_at_destination_even_with_late_frame() {
        assert_eq!(slide_x(3840, 3315, Duration::ZERO), 3840);
        assert_eq!(slide_x(3840, 3315, SLIDE), 3315);
        assert_eq!(slide_x(3840, 3315, SLIDE * 3), 3315);
    }
}
