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
    /// Opened by shortcut: focus controls closing. `focused` records whether focus has arrived.
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
    /// Periodic cursor reading. `editing` indicates active editing in the UI, page, or native picker.
    /// `escape` is a new Escape press.
    Tick {
        zone: CursorZone,
        editing: bool,
        focus: Focus,
        escape: bool,
    },
    Pin,
    Unpin,
    /// Global shortcut. `dialog` indicates that the native picker is open.
    Shortcut {
        dialog: bool,
    },
    /// Windows confirmed the focus requested by `Summon`.
    Activated,
    /// The UI left editing by keyboard while the cursor was outside the panel.
    EditingLeftOutside,
}

/// Effect for the calling layer to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Show,
    /// Shows the panel and gives it keyboard focus.
    Summon,
    Hide,
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
            (State::Hidden | State::Arming { .. } | State::Dismissed, Input::Shortcut { .. }) => {
                self.state = State::Summoned { focused: false };
                Action::Summon
            }
            (State::Open { .. } | State::Summoned { .. }, Input::Shortcut { .. }) => self.dismiss(),
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
            (State::Open { leaving_since }, Input::Tick { zone, editing, .. }) => {
                // The edge counts as inside because the panel touches it.
                let outside = zone == CursorZone::Outside && !editing;
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
            (
                State::Summoned { focused },
                Input::Tick {
                    editing,
                    focus,
                    escape,
                    ..
                },
            ) => match focus {
                Focus::Panel if escape && !editing => self.dismiss(),
                Focus::Panel | Focus::Page => {
                    self.state = State::Summoned { focused: true };
                    Action::None
                }
                // Outside the panel, only the native picker counts as editing.
                Focus::Elsewhere if editing => Action::None,
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
            // The user is working by keyboard: keep the panel as if summoned, so the next Escape or
            // a focus change closes it, instead of closing at once under a cursor that left long ago.
            (State::Open { .. }, Input::EditingLeftOutside) => {
                self.state = State::Summoned { focused: true };
                Action::None
            }
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

    fn tick(p: &mut Panel, zone: CursorZone, editing: bool, t0: Instant, ms: u64) -> Action {
        p.step(
            Input::Tick {
                zone,
                editing,
                focus: Focus::Elsewhere,
                escape: false,
            },
            t0 + Duration::from_millis(ms),
        )
    }

    /// Poll with the cursor outside, where shortcut opening differs from cursor opening.
    fn focus_tick(p: &mut Panel, focus: Focus, editing: bool, escape: bool, t0: Instant) -> Action {
        let tick = Input::Tick {
            zone: CursorZone::Outside,
            editing,
            focus,
            escape,
        };
        p.step(tick, t0)
    }

    fn shortcut(p: &mut Panel, t0: Instant) -> Action {
        p.step(Input::Shortcut { dialog: false }, t0)
    }

    fn summoned(t0: Instant) -> Panel {
        let mut p = Panel::new(D);
        assert_eq!(shortcut(&mut p, t0), Action::Summon);
        assert_eq!(
            focus_tick(&mut p, Focus::Panel, false, false, t0),
            Action::None
        );
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
    fn pinning_and_editing_prevent_closing() {
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

        // Leaving the editor with Escape away from the panel keeps it until a second Escape.
        let mut p = opened(t0);
        tick(&mut p, CursorZone::Outside, true, t0, 200);
        p.step(Input::EditingLeftOutside, t0 + Duration::from_millis(300));
        assert_eq!(
            focus_tick(
                &mut p,
                Focus::Panel,
                false,
                false,
                t0 + Duration::from_secs(5)
            ),
            Action::None
        );
        assert_ne!(
            focus_tick(
                &mut p,
                Focus::Panel,
                false,
                true,
                t0 + Duration::from_secs(6)
            ),
            Action::None
        );
    }

    #[test]
    fn shortcut_opens_during_edge_dwell() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        tick(&mut p, CursorZone::Edge, false, t0, 0);
        assert_eq!(shortcut(&mut p, t0), Action::Summon);
    }

    #[test]
    fn shortcut_opened_panel_ignores_cursor_outside() {
        let t0 = Instant::now();
        let mut p = summoned(t0);
        for ms in [0, 1000, 60_000] {
            let at = t0 + Duration::from_millis(ms);
            assert_eq!(
                focus_tick(&mut p, Focus::Panel, false, false, at),
                Action::None
            );
        }
    }

    #[test]
    fn escape_only_closes_when_panel_focused_and_no_field_is_being_edited() {
        let t0 = Instant::now();
        let mut p = summoned(t0);
        assert_eq!(
            focus_tick(&mut p, Focus::Panel, true, true, t0),
            Action::None
        );
        assert_eq!(
            focus_tick(&mut p, Focus::Page, true, true, t0),
            Action::None
        );
        assert_eq!(
            focus_tick(&mut p, Focus::Panel, false, true, t0),
            Action::Hide
        );
    }

    #[test]
    fn focus_on_another_app_closes_but_system_transition_does_not() {
        let t0 = Instant::now();
        let mut p = summoned(t0);
        assert_eq!(
            focus_tick(&mut p, Focus::Page, true, false, t0),
            Action::None
        );
        assert_eq!(
            focus_tick(&mut p, Focus::Unknown, false, false, t0),
            Action::None
        );
        // The native picker is foreground, but belongs to the panel.
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, true, false, t0),
            Action::None
        );
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, false, false, t0),
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
            focus_tick(&mut p, Focus::Elsewhere, false, false, t0),
            Action::Hide
        );
    }

    #[test]
    fn failed_activation_falls_back_to_cursor_rule() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        shortcut(&mut p, t0);
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, false, false, t0),
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

    #[test]
    fn shortcut_closes_and_edge_reopens_only_after_cursor_leaves() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        assert_eq!(shortcut(&mut p, t0), Action::Hide);
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
    fn shortcut_does_not_close_when_pinned_or_picker_is_open() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        assert_eq!(p.step(Input::Shortcut { dialog: true }, t0), Action::None);
        p.step(Input::Pin, t0);
        assert_eq!(shortcut(&mut p, t0), Action::None);
        assert_eq!(p.state, State::Pinned);
    }
}
