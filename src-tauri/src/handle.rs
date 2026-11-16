//! Pure rules for the edge handle: where it sits, when it shows and which stretch of the edge opens
//! the panel. Coordinates are physical pixels; sizes are logical and scale with the monitor's DPI.
//!
//! The handle is the panel's tab: it slides with the panel, outside its inner edge, so with the
//! panel parked beyond the screen it rests on the edge itself.

use crate::dock::Side;
use crate::panel::CursorZone;
use crate::platform::{Monitor, Point, Rect};

/// Depth of the tab beyond the panel's inner edge, as in the reference look.
const WIDTH: u32 = 20;
/// Tab height, its two curves included.
const HEIGHT: u32 = 152;
/// Height of each S-curve joining the panel's edge to the tab's outer side.
const CURVE: u32 = 28;
/// Length of the wordmark along the tab.
const WORDMARK: u32 = 52;
/// Edge beyond each end of the handle that still opens the panel.
const ACTIVATION_MARGIN: u32 = 16;
/// How near the edge, and the handle's ends, the cursor must come for the handle to show.
const REVEAL_DISTANCE: u32 = 48;
const BASE_DPI: u32 = 96;

fn scale(monitor: &Monitor, logical: u32) -> i32 {
    scale_dpi(monitor.dpi, logical)
}

fn scale_dpi(dpi: u32, logical: u32) -> i32 {
    (logical * dpi / BASE_DPI) as i32
}

/// The tab's drawing at one DPI, in physical pixels, as if the panel were on the right: the tab
/// grows left from the panel's edge at `x = depth`, and the window reaches `overlap` into it: just
/// the columns of the panel's line and focus band, which it repaints along the curves so no seam
/// shows, and no further, so the panel's resize strip beyond them keeps its clicks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub depth: i32,
    pub overlap: i32,
    pub height: i32,
    pub curve: f32,
    /// The panel's 1 px CSS border: Chromium floors border widths to device pixels, at least 1.
    pub line: i32,
    /// The panel's 2 px focus band, a positioned box whose edges Chromium rounds to pixels.
    pub band: i32,
    pub wordmark: f32,
}

impl Shape {
    pub fn new(dpi: u32) -> Self {
        let scale = dpi as f32 / BASE_DPI as f32;
        let band = ((2 * dpi + BASE_DPI / 2) / BASE_DPI) as i32;
        Self {
            depth: scale_dpi(dpi, WIDTH),
            overlap: band,
            height: scale_dpi(dpi, HEIGHT),
            curve: CURVE as f32 * scale,
            line: (dpi / BASE_DPI).max(1) as i32,
            band,
            wordmark: WORDMARK as f32 * scale,
        }
    }

    pub fn width(&self) -> i32 {
        self.depth + self.overlap
    }

    /// The window's columns that show the tab, without the strip over the panel.
    pub fn tab_columns(&self, side: Side) -> std::ops::Range<i32> {
        match side {
            Side::Right => 0..self.depth,
            Side::Left => self.overlap..self.width(),
        }
    }
}

/// The tab outside the inner edge of `panel`, wherever the panel is, vertically centered on the
/// work area like the open panel.
pub fn tab_rect(monitor: &Monitor, panel: Rect, side: Side) -> Rect {
    let Shape {
        depth: width,
        height,
        ..
    } = Shape::new(monitor.dpi);
    let top = monitor.work.top + (monitor.work.bottom - monitor.work.top - height) / 2;
    let (left, right) = match side {
        Side::Right => (panel.left - width, panel.left),
        Side::Left => (panel.right, panel.right + width),
    };
    Rect {
        left,
        top,
        right,
        bottom: top + height,
    }
}

/// The tab's window: the tab plus the strip of the panel's edge it covers.
pub fn window_rect(monitor: &Monitor, panel: Rect, side: Side) -> Rect {
    let tab = tab_rect(monitor, panel, side);
    let overlap = Shape::new(monitor.dpi).overlap;
    match side {
        Side::Right => Rect {
            right: tab.right + overlap,
            ..tab
        },
        Side::Left => Rect {
            left: tab.left - overlap,
            ..tab
        },
    }
}

/// The handle on the screen edge: where the tab rests while the panel is parked beyond it.
fn handle_rect(monitor: &Monitor, side: Side) -> Rect {
    let edge = match side {
        Side::Right => Rect {
            left: monitor.bounds.right,
            right: monitor.bounds.right,
            ..monitor.work
        },
        Side::Left => Rect {
            left: monitor.bounds.left,
            right: monitor.bounds.left,
            ..monitor.work
        },
    };
    tab_rect(monitor, edge, side)
}

/// The outer pixel column of the monitor.
fn edge_x(monitor: &Monitor, side: Side) -> i32 {
    match side {
        Side::Right => monitor.bounds.right - 1,
        Side::Left => monitor.bounds.left,
    }
}

/// Translates the cursor position. Only the edge column beside the handle, with a small margin above
/// and below, opens the panel; the tab counts as part of the open panel `panel`. Points outside the
/// monitor never open or hold the panel.
pub fn cursor_zone(cursor: Point, monitor: &Monitor, panel: Rect, side: Side) -> CursorZone {
    let handle = handle_rect(monitor, side);
    let margin = scale(monitor, ACTIVATION_MARGIN);
    let stretch = handle.top - margin..handle.bottom + margin;
    if !monitor.bounds.contains(cursor) {
        CursorZone::Outside
    } else if cursor.x == edge_x(monitor, side) && stretch.contains(&cursor.y) {
        CursorZone::Edge
    } else if panel.contains(cursor) || tab_rect(monitor, panel, side).contains(cursor) {
        CursorZone::Inside
    } else {
        CursorZone::Outside
    }
}

/// Whether the cursor is near enough for the closed panel's handle to show: within the reveal
/// distance of the edge, and of the handle's ends vertically, on this monitor.
pub fn revealed(cursor: Point, monitor: &Monitor, side: Side) -> bool {
    let handle = handle_rect(monitor, side);
    let reach = scale(monitor, REVEAL_DISTANCE);
    let near = Rect {
        top: handle.top - reach,
        bottom: handle.bottom + reach,
        ..match side {
            Side::Right => Rect {
                left: monitor.bounds.right - reach,
                ..handle
            },
            Side::Left => Rect {
                right: monitor.bounds.left + reach,
                ..handle
            },
        }
    };
    monitor.bounds.contains(cursor) && near.contains(cursor)
}

/// How the handle window behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Hidden,
    /// Shown, letting clicks through to the scrollbars and buttons below it.
    Through,
    /// The resting open panel's tab: a click closes the panel.
    Tab,
}

/// The handle shows while the panel is open or sliding, and while closed only with the cursor
/// near. Only the tab of the open panel at rest takes clicks: a tab sliding under the cursor, or
/// the closed panel's handle, never does. Nothing shows before the UI reports its look (`ready`).
pub fn presence(ready: bool, open: bool, sliding: bool, revealed: bool) -> Presence {
    if !ready || !(open || sliding || revealed) {
        Presence::Hidden
    } else if open && !sliding {
        Presence::Tab
    } else {
        Presence::Through
    }
}

/// A press on the tab: it stays a click only while the pointer never leaves the drag
/// tolerance (`slop`), so dragging away and back before releasing does nothing.
#[derive(Debug, Clone, Copy)]
pub struct Press {
    origin: Point,
    dragged: bool,
}

impl Press {
    pub fn new(origin: Point) -> Self {
        Self {
            origin,
            dragged: false,
        }
    }

    pub fn moved(&mut self, to: Point, slop: Point) {
        self.dragged |=
            (to.x - self.origin.x).abs() > slop.x || (to.y - self.origin.y).abs() > slop.y;
    }

    pub fn is_click(mut self, release: Point, slop: Point) -> bool {
        self.moved(release, slop);
        !self.dragged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3840×2160 at 150%, taskbar at the bottom. The handle is 30×228, its top at 930.
    const MONITOR: Monitor = Monitor {
        bounds: Rect {
            left: 0,
            top: 0,
            right: 3840,
            bottom: 2160,
        },
        work: Rect {
            left: 0,
            top: 0,
            right: 3840,
            bottom: 2088,
        },
        dpi: 144,
    };

    fn open_panel(side: Side) -> Rect {
        match side {
            Side::Right => Rect {
                left: 3150,
                ..MONITOR.work
            },
            Side::Left => Rect {
                right: 690,
                ..MONITOR.work
            },
        }
    }

    fn zone(x: i32, y: i32, side: Side) -> CursorZone {
        cursor_zone(Point { x, y }, &MONITOR, open_panel(side), side)
    }

    /// Catches the whole column opening the panel again, as when reaching for a scrollbar at the
    /// edge, or the stretch drifting away from the drawn handle.
    #[test]
    fn only_the_edge_beside_the_handle_opens() {
        // The handle spans 930..1158 and the margin is 24 pixels on each side.
        for (y, opens) in [
            (0, false),
            (905, false),
            (906, true),
            (1040, true),
            (1181, true),
            (1182, false),
            (2159, false),
        ] {
            assert_eq!(
                zone(3839, y, Side::Right) == CursorZone::Edge,
                opens,
                "y={y}"
            );
            assert_eq!(zone(0, y, Side::Left) == CursorZone::Edge, opens, "y={y}");
        }
    }

    #[test]
    fn only_the_outer_column_of_the_panel_monitor_opens() {
        assert_eq!(zone(3838, 1040, Side::Right), CursorZone::Inside);
        assert_eq!(zone(1, 1040, Side::Left), CursorZone::Inside);
        assert_eq!(zone(3839, 1040, Side::Left), CursorZone::Outside);
        assert_eq!(zone(0, 1040, Side::Right), CursorZone::Outside);
        // A neighboring monitor's column beyond the edge does not count.
        assert_eq!(zone(3840, 1040, Side::Right), CursorZone::Outside);
        assert_eq!(zone(-1, 1040, Side::Left), CursorZone::Outside);
    }

    /// Catches a hover-opened panel closing while the cursor goes to its tab to click it.
    #[test]
    fn the_tab_belongs_to_the_open_panel() {
        assert_eq!(zone(3149, 1040, Side::Right), CursorZone::Inside);
        assert_eq!(zone(3120, 1040, Side::Right), CursorZone::Inside);
        assert_eq!(zone(3119, 1040, Side::Right), CursorZone::Outside);
        assert_eq!(zone(3149, 929, Side::Right), CursorZone::Outside);
        assert_eq!(zone(690, 1040, Side::Left), CursorZone::Inside);
        assert_eq!(zone(719, 1040, Side::Left), CursorZone::Inside);
        assert_eq!(zone(720, 1040, Side::Left), CursorZone::Outside);
    }

    /// Catches the handle showing in one place while the edge opens in another: the tab of the
    /// parked panel, which slides with it, must rest exactly on the handle.
    #[test]
    fn the_parked_panel_tab_rests_on_the_handle() {
        let width = 690;
        let right = Rect {
            left: MONITOR.bounds.right,
            right: MONITOR.bounds.right + width,
            ..MONITOR.work
        };
        let left = Rect {
            left: MONITOR.bounds.left - width,
            right: MONITOR.bounds.left,
            ..MONITOR.work
        };
        for (panel, side) in [(right, Side::Right), (left, Side::Left)] {
            let tab = tab_rect(&MONITOR, panel, side);
            assert_eq!(tab, handle_rect(&MONITOR, side));
            assert!(MONITOR.bounds.contains(Point {
                x: edge_x(&MONITOR, side),
                y: tab.top
            }));
            assert_eq!((tab.right - tab.left, tab.bottom - tab.top), (30, 228));
        }
    }

    /// Catches the handle staying hidden where it opens, or showing over the whole edge.
    #[test]
    fn the_handle_shows_only_near_the_edge_and_its_ends() {
        let near = |x, y, side| revealed(Point { x, y }, &MONITOR, side);
        // The reveal distance is 72 pixels: from the edge, and beyond each end of the handle.
        for y in [858, 1040, 1229] {
            assert!(near(3839, y, Side::Right), "y={y}");
            assert!(near(3768, y, Side::Right), "y={y}");
            assert!(near(0, y, Side::Left), "y={y}");
            assert!(near(71, y, Side::Left), "y={y}");
        }
        assert!(!near(3767, 1040, Side::Right));
        assert!(!near(72, 1040, Side::Left));
        for y in [857, 1230, 0, 2159] {
            assert!(!near(3839, y, Side::Right), "y={y}");
            assert!(!near(0, y, Side::Left), "y={y}");
        }
        // Off the panel's monitor.
        assert!(!near(3840, 1040, Side::Right));
        assert!(!near(-1, 1040, Side::Left));
    }

    /// Catches the handle following the monitor instead of the work area, or forgetting the DPI.
    #[test]
    fn the_handle_centers_on_the_work_area_at_any_scale() {
        for (dpi, size) in [(96, (20, 152)), (120, (25, 190)), (144, (30, 228))] {
            let monitor = Monitor {
                dpi,
                work: Rect {
                    top: 100,
                    ..MONITOR.work
                },
                ..MONITOR
            };
            let handle = handle_rect(&monitor, Side::Right);
            let (width, height) = size;
            assert_eq!(handle.right, 3840);
            assert_eq!(handle.right - handle.left, width);
            assert_eq!(handle.bottom - handle.top, height);
            assert_eq!(handle.top - 100, 2088 - handle.bottom);
        }
    }

    /// Catches the closed panel's handle, or a tab sliding under the cursor, taking a click meant
    /// for a scrollbar or a maximized window's buttons.
    #[test]
    fn only_the_resting_open_tab_takes_clicks() {
        use Presence::{Hidden, Tab, Through};
        // (open, sliding, revealed)
        for (state, expected) in [
            ((false, false, false), Hidden),
            ((false, false, true), Through),
            ((false, true, false), Through),
            ((true, true, false), Through),
            ((true, true, true), Through),
            ((true, false, false), Tab),
            ((true, false, true), Tab),
        ] {
            let (open, sliding, revealed) = state;
            assert_eq!(
                presence(true, open, sliding, revealed),
                expected,
                "{state:?}"
            );
            assert_eq!(
                presence(false, open, sliding, revealed),
                Hidden,
                "{state:?}"
            );
        }
    }

    #[test]
    fn dragging_away_and_back_is_not_a_click() {
        let slop = Point { x: 4, y: 4 };
        let mut press = Press::new(Point { x: 10, y: 40 });
        press.moved(Point { x: 10, y: 90 }, slop);
        press.moved(Point { x: 10, y: 40 }, slop);
        assert!(!press.is_click(Point { x: 10, y: 40 }, slop));
        assert!(Press::new(Point { x: 10, y: 40 }).is_click(Point { x: 12, y: 41 }, slop));
    }
}
