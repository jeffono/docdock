//! Connects the state machine to the window: one thread polls the cursor, receives UI requests,
//! and animates sliding. A single thread avoids races between opening, closing, and animation.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::panel::{Action, CursorZone, Delays, Focus, Input, Panel, State};
use crate::platform::{self, Activation, Monitor, PanelWindow, Point, Rect};

const POLL: Duration = Duration::from_millis(50);
const SLIDE: Duration = Duration::from_millis(150);
/// Width in logical pixels; on the monitor it becomes `width * dpi / 96`.
const PANEL_WIDTH: u32 = 420;
const BASE_DPI: u32 = 96;
/// Two bars above the page, in logical pixels: twice `--size-bar` in `tokens.css`.
const WEB_TOP: u32 = 88;

/// UI requests to the panel thread.
#[derive(Debug, Clone, Copy)]
pub enum Request {
    Activate,
    SetPinned(bool),
    SetEditing(bool),
    SetDialog(bool),
    /// The web page window was created and is still invisible.
    AttachWeb(isize),
    /// Global shortcut.
    Shortcut,
}

/// Open panel rectangle, aligned with the right edge of the work area.
fn panel_rect(monitor: &Monitor) -> Rect {
    let width = (PANEL_WIDTH * monitor.dpi / BASE_DPI) as i32;
    Rect {
        left: monitor.work.right - width,
        ..monitor.work
    }
}

/// Web page rectangle: the panel minus the top bar.
fn web_rect(monitor: &Monitor, panel: Rect) -> Rect {
    Rect {
        top: panel.top + (WEB_TOP * monitor.dpi / BASE_DPI) as i32,
        ..panel
    }
}

/// Only the right edge of the rightmost monitor holds the cursor; elsewhere it would cross into a
/// neighboring monitor. It also puts the parked position outside every screen. On a tie, such as
/// stacked monitors, prefer the primary monitor, whose origin is (0, 0).
fn edge_monitor(monitors: Vec<Monitor>) -> Option<Monitor> {
    monitors.into_iter().max_by_key(|monitor| {
        let primary = monitor.bounds.left == 0 && monitor.bounds.top == 0;
        (monitor.bounds.right, primary)
    })
}

/// The new panel monitor if a layout, resolution, DPI, or taskbar change changed the current one.
/// If no monitors are available, as during a transition, retain the current one.
fn monitor_change(current: &Monitor, monitors: Vec<Monitor>) -> Option<Monitor> {
    edge_monitor(monitors).filter(|monitor| monitor != current)
}

/// Translates the cursor position. Points outside the monitor never open or hold the panel.
fn cursor_zone(cursor: Point, monitor: Rect, panel: Rect) -> CursorZone {
    if !monitor.contains(cursor) {
        CursorZone::Outside
    } else if cursor.x == monitor.right - 1 {
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
    window: PanelWindow,
    /// Web page window, if present.
    web: Option<PanelWindow>,
    monitor: Monitor,
    rect: Rect,
    parked_x: i32,
    x: i32,
    panel: Panel,
    editing: bool,
    dialog: bool,
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

    /// Moves windows to the new monitor without animation. Any ongoing slide used the old monitor.
    fn relocate(&mut self, monitor: Monitor) {
        eprintln!("[panel] monitor changed: {monitor:?}");
        self.monitor = monitor;
        self.rect = panel_rect(&monitor);
        self.parked_x = monitor.bounds.right;
        self.slide = None;
        self.x = if self.is_open() {
            self.rect.left
        } else {
            self.parked_x
        };
        self.window.place(self.rect, self.parked_x, self.x);
        if let Some(web) = &mut self.web {
            web.place(web_rect(&monitor, self.rect), self.parked_x, self.x);
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
            Request::SetEditing(editing) => {
                eprintln!("[panel] editing={editing}");
                self.editing = editing;
                self.spend_escape();
                Action::None
            }
            Request::SetDialog(dialog) => {
                self.dialog = dialog;
                self.spend_escape();
                Action::None
            }
            Request::AttachWeb(raw) => {
                let rect = web_rect(&self.monitor, self.rect);
                let right = self.monitor.bounds.right;
                let web = PanelWindow::prepare(raw, rect, right, self.x, Activation::OnClick);
                web.set_enabled(self.is_open());
                self.web = Some(web);
                Action::None
            }
            Request::SetPinned(true) => self.panel.step(Input::Pin, now),
            Request::SetPinned(false) => self.panel.step(Input::Unpin, now),
            Request::Shortcut => {
                let dialog = self.dialog;
                self.panel.step(Input::Shortcut { dialog }, now)
            }
        };
        self.apply(action);
    }

    fn poll(&mut self) {
        let Some(cursor) = platform::cursor_pos() else {
            return;
        };
        if let Some(monitor) = monitor_change(&self.monitor, platform::monitors()) {
            self.relocate(monitor);
        }
        self.track_focus();
        let zone = cursor_zone(cursor, self.monitor.bounds, self.rect);
        // A user typing on the page can move the mouse away without losing the panel. The page does
        // not talk to Rust, so Escape is read from the keyboard; over the page it remains the page's.
        // Once focus is restored, the panel closes as with any other focus loss.
        let focus = self.focus();
        let page_focused = focus == Focus::Page;
        let escape = self.escape_tap();
        if page_focused && zone == CursorZone::Outside && escape {
            platform::restore_focus(self.previous_focus);
        }
        // The field can remain focused in the document after the user switches apps.
        let editing = self.editing && focus == Focus::Panel;
        let tick = Input::Tick {
            zone,
            editing: editing || page_focused || self.dialog,
            focus,
            escape,
        };
        let action = self.panel.step(tick, Instant::now());
        self.apply(action);
    }

    fn focus(&self) -> Focus {
        if self.window.is_foreground() {
            Focus::Panel
        } else if self.web.is_some_and(|web| web.is_foreground()) {
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

    /// Escape leaving a field or closing the picker arrives before the poll. It was already handled:
    /// counting it would also close the panel with the same press.
    fn spend_escape(&mut self) {
        self.escape_spent = platform::escape_pressed();
    }

    /// Panel windows. The page window disappears from the list if destroyed, for example by Alt+F4.
    fn windows(&mut self) -> Vec<PanelWindow> {
        self.web = self.web.filter(PanelWindow::exists);
        std::iter::once(self.window).chain(self.web).collect()
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
        self.slide = Some(Slide::new(self.x, self.rect.left));
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
                // If the user already clicked another app, leave its focus alone.
                if self.is_foreground() {
                    platform::restore_focus(self.previous_focus);
                }
                self.set_enabled(false);
                self.slide = Some(Slide::new(self.x, self.parked_x));
            }
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
pub fn start(raw_window: isize, requests: Receiver<Request>) {
    let monitor = edge_monitor(platform::monitors()).expect("no monitor available");
    let rect = panel_rect(&monitor);
    let parked_x = monitor.bounds.right;
    let right = monitor.bounds.right;
    let window = PanelWindow::prepare(raw_window, rect, right, parked_x, Activation::OnRequest);
    eprintln!("[panel] monitor {monitor:?}, panel {rect:?}");
    let dock = Dock {
        window,
        web: None,
        monitor,
        rect,
        parked_x,
        x: parked_x,
        panel: Panel::new(Delays::default()),
        editing: false,
        dialog: false,
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

    fn zone(x: i32, y: i32) -> CursorZone {
        cursor_zone(Point { x, y }, PRIMARY, PANEL)
    }

    #[test]
    fn panel_stays_on_rightmost_monitor() {
        let principal = monitor(0, 3840);
        let chosen = edge_monitor(vec![principal, monitor(3840, 7680)]).unwrap();
        assert_eq!(chosen.bounds.right, 7680);
        let chosen = edge_monitor(vec![monitor(-3840, 0), principal]).unwrap();
        assert_eq!(chosen.bounds.right, 3840);
    }

    #[test]
    fn swapping_monitors_while_app_is_open_moves_panel() {
        let atual = monitor(3840, 7680);
        let invertido = vec![monitor(-3840, 0), monitor(0, 3840)];
        let novo = monitor_change(&atual, invertido).unwrap();
        assert_eq!(novo.bounds.right, 3840);
        assert!(monitor_change(&atual, vec![monitor(0, 3840), atual]).is_none());
        assert!(monitor_change(&atual, Vec::new()).is_none());
    }

    #[test]
    fn only_last_monitor_pixel_is_edge() {
        assert_eq!(zone(3839, 0), CursorZone::Edge);
        assert_eq!(zone(3839, 2159), CursorZone::Edge);
        assert_eq!(zone(3838, 0), CursorZone::Outside);
    }

    #[test]
    fn points_on_other_monitors_are_ignored() {
        assert_eq!(zone(-1, 500), CursorZone::Outside);
    }

    #[test]
    fn slide_ends_exactly_at_destination_even_with_late_frame() {
        assert_eq!(slide_x(3840, 3315, Duration::ZERO), 3840);
        assert_eq!(slide_x(3840, 3315, SLIDE), 3315);
        assert_eq!(slide_x(3840, 3315, SLIDE * 3), 3315);
    }
}
