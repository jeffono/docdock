//! Pure panel state machine: no Tauri, Win32, or clock.
//!
//! The platform layer polls the cursor (~50 ms), translates its position to [`CursorZone`],
//! and calls [`Panel::step`] with the current instant. The output describes the effect to perform.

use std::time::{Duration, Instant};

/// Panel delays.
#[derive(Debug, Clone, Copy)]
pub struct Delays {
    /// Continuous dwell at the edge before opening.
    pub open: Duration,
    /// Time outside the panel before closing.
    pub close: Duration,
}

impl Default for Delays {
    fn default() -> Self {
        Self {
            open: Duration::from_millis(150),
            close: Duration::from_millis(400),
        }
    }
}

/// Cursor location as translated by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorZone {
    /// Touching the activation edge.
    Edge,
    /// Inside the open panel.
    Inside,
    /// Anywhere else.
    Outside,
}

/// Keyboard focus as translated by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The panel window.
    Panel,
    /// The panel's web page.
    Page,
    /// Another app.
    Elsewhere,
    /// Nobody during system transitions; this does not prove focus was lost.
    Unknown,
}

/// Panel state. `Arming` and `Open` store when the wait began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Hidden,
    Arming {
        since: Instant,
    },
    /// `leaving_since` is `Some` while the cursor is outside and the close delay is counting.
    Open {
        leaving_since: Option<Instant>,
    },
    /// Opened by shortcut, or given focus by a click while open: focus controls closing. `focused`
    /// records whether focus has arrived.
    Summoned {
        focused: bool,
    },
    Pinned,
    /// Closed manually. The edge is rearmed only after the cursor leaves it; otherwise a stationary
    /// cursor would reopen the panel immediately after closing.
    Dismissed,
}

/// State machine input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// Periodic cursor reading. `dialog` indicates that the native picker is open.
    Tick {
        zone: CursorZone,
        dialog: bool,
        focus: Focus,
    },
    Pin,
    Unpin,
    /// Global shortcut. `dialog` indicates that the native picker is open.
    Shortcut {
        dialog: bool,
    },
    /// Windows confirmed the focus requested by `Summon`.
    Activated,
    /// The UI asked to close: Escape in the search bar, or the shortcut while already there. A pinned
    /// panel only gives up focus.
    Close,
}

/// Effect for the calling layer to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Show,
    /// Shows the panel and gives it keyboard focus.
    Summon,
    Hide,
    /// Returns keyboard focus to the previous app; the panel stays shown.
    Release,
}

#[derive(Debug, Clone, Copy)]
pub struct Panel {
    pub state: State,
    pub delays: Delays,
}

impl Panel {
    pub fn new(delays: Delays) -> Self {
        Self {
            state: State::Hidden,
            delays,
        }
    }

    fn dismiss(&mut self) -> Action {
        self.state = State::Dismissed;
        Action::Hide
    }

    /// Advances the machine and returns the effect to perform.
    pub fn step(&mut self, input: Input, now: Instant) -> Action {
        match (self.state, input) {
            // The picker is modal: closing it would re-enable the already hidden panel window.
            (_, Input::Shortcut { dialog: true }) => Action::None,
            // Pinned stays pinned: the shortcut only brings focus to the search bar.
            (State::Pinned, Input::Shortcut { .. }) => Action::Summon,
            // An open panel takes focus instead of closing; the UI closes it when the shortcut
            // arrives with focus already in the search bar.
            (_, Input::Shortcut { .. }) => {
                self.state = State::Summoned { focused: false };
                Action::Summon
            }
            (State::Open { .. } | State::Summoned { .. }, Input::Close) => self.dismiss(),
            (State::Pinned, Input::Close) => Action::Release,
            (State::Dismissed, Input::Tick { zone, .. }) => {
                if zone != CursorZone::Edge {
                    self.state = State::Hidden;
                }
                Action::None
            }
            (
                State::Hidden,
                Input::Tick {
                    zone: CursorZone::Edge,
                    ..
                },
            ) => {
                self.state = State::Arming { since: now };
                Action::None
            }
            (State::Arming { since }, Input::Tick { zone, .. }) => match zone {
                CursorZone::Edge if now.duration_since(since) >= self.delays.open => {
                    self.state = State::Open {
                        leaving_since: None,
                    };
                    Action::Show
                }
                CursorZone::Edge => Action::None,
                _ => {
                    self.state = State::Hidden;
                    Action::None
                }
            },
            // A click gave the panel or its page focus: from now on focus governs, as if summoned,
            // so the cursor leaving no longer closes it under the user's typing.
            (
                State::Open { .. },
                Input::Tick {
                    focus: Focus::Panel | Focus::Page,
                    ..
                },
            ) => {
                self.state = State::Summoned { focused: true };
                Action::None
            }
            (State::Open { leaving_since }, Input::Tick { zone, dialog, .. }) => {
                // The edge counts as inside because the panel touches it.
                let outside = zone == CursorZone::Outside && !dialog;
                match (outside, leaving_since) {
                    (false, _) => {
                        self.state = State::Open {
                            leaving_since: None,
                        };
                        Action::None
                    }
                    (true, None) => {
                        self.state = State::Open {
                            leaving_since: Some(now),
                        };
                        Action::None
                    }
                    (true, Some(t)) if now.duration_since(t) >= self.delays.close => {
                        self.state = State::Hidden;
                        Action::Hide
                    }
                    (true, Some(_)) => Action::None,
                }
            }
            (State::Summoned { focused }, Input::Tick { dialog, focus, .. }) => match focus {
                Focus::Panel | Focus::Page => {
                    self.state = State::Summoned { focused: true };
                    Action::None
                }
                // The native picker is foreground but belongs to the panel.
                Focus::Elsewhere if dialog => Action::None,
                Focus::Elsewhere if focused => self.dismiss(),
                // Activation failed. With no focus to lose, use the cursor rule so the panel does
                // not remain stuck above other windows.
                Focus::Elsewhere => {
                    self.state = State::Open {
                        leaving_since: None,
                    };
                    Action::None
                }
                Focus::Unknown => Action::None,
            },
            (State::Summoned { .. }, Input::Activated) => {
                self.state = State::Summoned { focused: true };
                Action::None
            }
            (State::Open { .. } | State::Summoned { .. }, Input::Pin) => {
                self.state = State::Pinned;
                Action::None
            }
            (State::Hidden | State::Arming { .. } | State::Dismissed, Input::Pin) => {
                self.state = State::Pinned;
                Action::Show
            }
            (State::Pinned, Input::Unpin) => {
                // Restart the delay: the cursor may already be outside.
                self.state = State::Open {
                    leaving_since: None,
                };
                Action::None
            }
            _ => Action::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Delays = Delays {
        open: Duration::from_millis(150),
        close: Duration::from_millis(400),
    };

    /// Cursor poll while another app has focus: the cursor rule.
    fn tick(p: &mut Panel, zone: CursorZone, dialog: bool, t0: Instant, ms: u64) -> Action {
        let tick = Input::Tick {
            zone,
            dialog,
            focus: Focus::Elsewhere,
        };
        p.step(tick, t0 + Duration::from_millis(ms))
    }

    /// Poll with the cursor outside, where focus governs and the cursor would close the panel.
    fn focus_tick(p: &mut Panel, focus: Focus, dialog: bool, t0: Instant, ms: u64) -> Action {
        let tick = Input::Tick {
            zone: CursorZone::Outside,
            dialog,
            focus,
        };
        p.step(tick, t0 + Duration::from_millis(ms))
    }

    fn shortcut(p: &mut Panel, t0: Instant) -> Action {
        p.step(Input::Shortcut { dialog: false }, t0)
    }

    fn summoned(t0: Instant) -> Panel {
        let mut p = Panel::new(D);
        assert_eq!(shortcut(&mut p, t0), Action::Summon);
        assert_eq!(focus_tick(&mut p, Focus::Panel, false, t0, 0), Action::None);
        p
    }

    fn opened(t0: Instant) -> Panel {
        let mut p = Panel::new(D);
        tick(&mut p, CursorZone::Edge, false, t0, 0);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 150), Action::Show);
        p
    }

    #[test]
    fn opening_requires_dwell_at_edge() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        tick(&mut p, CursorZone::Edge, false, t0, 0);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 100), Action::None);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 150), Action::Show);
    }

    #[test]
    fn quick_pass_over_edge_does_not_open() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        tick(&mut p, CursorZone::Edge, false, t0, 0);
        tick(&mut p, CursorZone::Outside, false, t0, 50);
        // Returning to the edge restarts the delay.
        tick(&mut p, CursorZone::Edge, false, t0, 100);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 200), Action::None);
        assert_eq!(
            p.state,
            State::Arming {
                since: t0 + Duration::from_millis(100)
            }
        );
    }

    #[test]
    fn reentry_cancels_closing() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        tick(&mut p, CursorZone::Outside, false, t0, 200);
        tick(&mut p, CursorZone::Inside, false, t0, 400);
        // Leaving again restarts the 400 ms delay.
        tick(&mut p, CursorZone::Outside, false, t0, 450);
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 800),
            Action::None
        );
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 850),
            Action::Hide
        );
    }

    #[test]
    fn pinning_and_picker_prevent_closing() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        tick(&mut p, CursorZone::Outside, true, t0, 200);
        assert_eq!(
            tick(&mut p, CursorZone::Outside, true, t0, 5000),
            Action::None
        );

        p.step(Input::Pin, t0 + Duration::from_millis(5001));
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 9000),
            Action::None
        );
        assert_eq!(p.state, State::Pinned);

        // Once unpinned, it closes normally again.
        p.step(Input::Unpin, t0 + Duration::from_millis(9001));
        tick(&mut p, CursorZone::Outside, false, t0, 9002);
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 9402),
            Action::Hide
        );
    }

    /// Bug caught: the user clicks the hover-opened panel to type, moves the mouse away, and the
    /// panel closes under the keyboard.
    #[test]
    fn click_focus_keeps_cursor_opened_panel_open_after_cursor_leaves() {
        for focus in [Focus::Panel, Focus::Page] {
            let t0 = Instant::now();
            let mut p = opened(t0);
            for ms in [200, 1000, 60_000] {
                assert_eq!(focus_tick(&mut p, focus, false, t0, ms), Action::None);
            }
            assert_eq!(p.state, State::Summoned { focused: true });
        }
    }

    /// Bug caught: a panel focused by a click stays over other windows after the user moves on.
    #[test]
    fn click_focused_panel_closes_when_focus_moves_to_another_app() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        focus_tick(&mut p, Focus::Panel, false, t0, 200);
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, false, t0, 250),
            Action::Hide
        );
    }

    #[test]
    fn shortcut_opens_during_edge_dwell() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        tick(&mut p, CursorZone::Edge, false, t0, 0);
        assert_eq!(shortcut(&mut p, t0), Action::Summon);
    }

    /// Bug caught: the shortcut closes a panel the user wanted to type in, opened by hover or
    /// focused outside the search bar. Closing from the search bar is the UI's `Close`.
    #[test]
    fn shortcut_on_open_panel_takes_focus_instead_of_closing() {
        let t0 = Instant::now();
        for mut p in [opened(t0), summoned(t0)] {
            assert_eq!(shortcut(&mut p, t0), Action::Summon);
            assert_eq!(p.state, State::Summoned { focused: false });
            p.step(Input::Activated, t0);
            assert_eq!(
                focus_tick(&mut p, Focus::Panel, false, t0, 60_000),
                Action::None
            );
        }
    }

    #[test]
    fn shortcut_opened_panel_ignores_cursor_outside() {
        let t0 = Instant::now();
        let mut p = summoned(t0);
        for ms in [0, 1000, 60_000] {
            assert_eq!(
                focus_tick(&mut p, Focus::Panel, false, t0, ms),
                Action::None
            );
        }
    }

    #[test]
    fn focus_on_another_app_closes_but_system_transition_does_not() {
        let t0 = Instant::now();
        let mut p = summoned(t0);
        assert_eq!(focus_tick(&mut p, Focus::Page, false, t0, 0), Action::None);
        assert_eq!(
            focus_tick(&mut p, Focus::Unknown, false, t0, 0),
            Action::None
        );
        // The native picker is foreground, but belongs to the panel.
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, true, t0, 0),
            Action::None
        );
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, false, t0, 0),
            Action::Hide
        );
    }

    #[test]
    fn focus_lost_before_next_poll_closes_panel() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        shortcut(&mut p, t0);
        p.step(Input::Activated, t0);
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, false, t0, 0),
            Action::Hide
        );
    }

    #[test]
    fn failed_activation_falls_back_to_cursor_rule() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        shortcut(&mut p, t0);
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, false, t0, 0),
            Action::None
        );
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 0),
            Action::None
        );
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 400),
            Action::Hide
        );
    }

    /// Bug caught: Escape in the empty search bar, or the shortcut there, leaves the panel open, or
    /// a stationary cursor at the edge reopens it at once.
    #[test]
    fn close_request_hides_and_edge_reopens_only_after_cursor_leaves() {
        let t0 = Instant::now();
        let mut p = summoned(t0);
        assert_eq!(p.step(Input::Close, t0), Action::Hide);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 200), Action::None);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 900), Action::None);
        assert_eq!(p.state, State::Dismissed);

        tick(&mut p, CursorZone::Outside, false, t0, 950);
        tick(&mut p, CursorZone::Edge, false, t0, 1000);
        assert_eq!(
            tick(&mut p, CursorZone::Edge, false, t0, 1150),
            Action::Show
        );
    }

    #[test]
    fn shortcut_does_nothing_while_picker_is_open() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        assert_eq!(p.step(Input::Shortcut { dialog: true }, t0), Action::None);
        assert_eq!(
            p.state,
            State::Open {
                leaving_since: None
            }
        );
    }

    /// Bug caught: the shortcut or Escape unpins the panel by closing it, or Escape keeps the
    /// keyboard in a pinned panel the user wanted to leave.
    #[test]
    fn pinned_panel_releases_focus_on_close_and_stays_shown() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        p.step(Input::Pin, t0);
        assert_eq!(shortcut(&mut p, t0), Action::Summon);
        p.step(Input::Activated, t0);
        assert_eq!(p.step(Input::Close, t0), Action::Release);
        // Focus back in the other app and the cursor away: still shown and pinned.
        for ms in [0, 1000, 60_000] {
            assert_eq!(
                focus_tick(&mut p, Focus::Elsewhere, false, t0, ms),
                Action::None
            );
        }
        assert_eq!(p.state, State::Pinned);
        // The shortcut takes focus back.
        assert_eq!(shortcut(&mut p, t0), Action::Summon);
        assert_eq!(p.state, State::Pinned);
    }
}
