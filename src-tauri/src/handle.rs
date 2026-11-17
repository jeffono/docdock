//! Pure rules for the edge handle: where it sits, when it shows and which stretch of the edge opens
//! the panel. Coordinates are physical pixels; sizes are logical and scale with the monitor's DPI.
//!
//! The handle is the panel's tab, which the panel's own WebView draws in a strip as deep as the tab
//! beside the panel: it slides with the panel, outside its inner edge, so with the panel parked
//! beyond the screen it rests on the edge itself.

use serde::Serialize;

use crate::dock::Side;
use crate::panel::CursorZone;
use crate::platform::{Monitor, Point, Rect};
use std::time::Duration;

/// Depth of the tab beyond the panel's inner edge, as in the reference look.
const DEPTH: u32 = 20;
/// Tab height, its two curves included.
const HEIGHT: u32 = 152;
/// Height of each S-curve joining the panel's edge to the tab's outer side.
const CURVE: u32 = 28;
/// Activation height as a percentage of the tab height.
pub const ACTIVATION_PERCENT: u32 = 300;
/// How near the tab the cursor must come for the handle to show, as a percentage of the tab height.
pub const REVEAL_PERCENT: u32 = 400;
/// Speed toward the edge, in logical pixels per second, from which an approach is a rush.
pub const RUSH_SPEED: u32 = 500;
const BASE_DPI: u32 = 96;

/// The values the development tweak window changes live; release builds use the constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuning {
    /// [`ACTIVATION_PERCENT`].
    pub activation: u32,
    /// [`REVEAL_PERCENT`].
    pub reveal: u32,
    /// [`RUSH_SPEED`].
    pub rush: u32,
    /// [`LOGICAL`].
    pub tab: Logical,
}

pub const TUNING: Tuning = Tuning {
    activation: ACTIVATION_PERCENT,
    reveal: REVEAL_PERCENT,
    rush: RUSH_SPEED,
    tab: LOGICAL,
};

/// The tab's size in logical pixels, which the UI draws it at: the window region below follows
/// the same outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Logical {
    pub depth: u32,
    pub height: u32,
    pub curve: u32,
}

pub const LOGICAL: Logical = Logical {
    depth: DEPTH,
    height: HEIGHT,
    curve: CURVE,
};

fn scale_dpi(dpi: u32, logical: u32) -> i32 {
    (logical * dpi / BASE_DPI) as i32
}

/// The tab at one DPI, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub depth: i32,
    pub height: i32,
    pub curve: f32,
}

impl Shape {
    pub fn new(tab: Logical, dpi: u32) -> Self {
        Self {
            depth: scale_dpi(dpi, tab.depth),
            height: scale_dpi(dpi, tab.height),
            curve: tab.curve as f32 * dpi as f32 / BASE_DPI as f32,
        }
    }
}

/// The tab outside the inner edge of `panel`, wherever the panel is, vertically centered on the
/// work area like the open panel.
fn tab_rect(monitor: &Monitor, panel: Rect, side: Side, tab: Logical) -> Rect {
    let Shape { depth, height, .. } = Shape::new(tab, monitor.dpi);
    let top = monitor.work.top + (monitor.work.bottom - monitor.work.top - height) / 2;
    let (left, right) = match side {
        Side::Right => (panel.left - depth, panel.left),
        Side::Left => (panel.right, panel.right + depth),
    };
    Rect {
        left,
        top,
        right,
        bottom: top + height,
    }
}

/// The panel's own window: the panel widened by the tab's depth on its inner side, where its
/// WebView draws the tab.
pub fn framed_rect(monitor: &Monitor, panel: Rect, side: Side, tab: Logical) -> Rect {
    let depth = Shape::new(tab, monitor.dpi).depth;
    match side {
        Side::Right => Rect {
            left: panel.left - depth,
            ..panel
        },
        Side::Left => Rect {
            right: panel.right + depth,
            ..panel
        },
    }
}

/// What of the widened window shows and takes clicks: the panel and the tab's outline, row by row,
/// a pixel wider than the outline so its antialiased edge shows. Elsewhere the window is
/// transparent, and clicks reach the windows below.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
    side: Side,
    shape: Shape,
    /// Panel width without the tab.
    width: i32,
    /// Window height.
    height: i32,
    /// The tab's top row in window coordinates.
    top: i32,
}

impl Outline {
    pub fn new(monitor: &Monitor, panel: Rect, side: Side, tab: Logical) -> Self {
        Self {
            side,
            shape: Shape::new(tab, monitor.dpi),
            width: panel.right - panel.left,
            height: panel.bottom - panel.top,
            top: tab_rect(monitor, panel, side, tab).top - panel.top,
        }
    }

    /// Distance from the tab's outer side to its boundary `y` rows into the tab: the panel's edge
    /// (`depth`) beyond the tab, 0 along its outer side, and the S-curves between.
    fn boundary(&self, y: f32) -> f32 {
        let Shape {
            depth,
            height,
            curve,
        } = self.shape;
        let (depth, height) = (depth as f32, height as f32);
        let y = y.min(height - y);
        if y <= 0.0 {
            return depth;
        }
        if y >= curve {
            return 0.0;
        }
        // The curve from (depth, 0) through (depth, c/2) and (0, c/2) to (0, c), as the UI draws
        // it: y(t) = c(1.5t(1−t) + t³) rises with t, so bisect for it.
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        for _ in 0..24 {
            let t = (low + high) / 2.0;
            if curve * (1.5 * t * (1.0 - t) + t * t * t) < y {
                low = t;
            } else {
                high = t;
            }
        }
        let t = (low + high) / 2.0;
        depth * (1.0 - t) * (1.0 - t) * (1.0 + 2.0 * t)
    }

    /// Window rectangles, rows of equal extent merged. One row of slack above and below covers the
    /// UI centering the tab on a fraction of a pixel.
    pub fn rects(&self) -> Vec<Rect> {
        let depth = self.shape.depth;
        let half = self.shape.height as f32 / 2.0;
        let mut rects: Vec<Rect> = Vec::new();
        for row in 0..self.height {
            let (from, to) = ((row - self.top - 1) as f32, (row - self.top + 2) as f32);
            // The outermost point of the boundary within these rows.
            let outer = if to <= half {
                self.boundary(to)
            } else if from >= half {
                self.boundary(from)
            } else {
                0.0
            };
            let reach = depth - outer.floor() as i32;
            let (left, right) = match self.side {
                Side::Right => (depth - reach, depth + self.width),
                Side::Left => (0, self.width + reach),
            };
            match rects.last_mut() {
                Some(last) if last.left == left && last.right == right && last.bottom == row => {
                    last.bottom = row + 1;
                }
                _ => rects.push(Rect {
                    left,
                    top: row,
                    right,
                    bottom: row + 1,
                }),
            }
        }
        rects
    }
}

/// The handle on the screen edge: where the tab rests while the panel is parked beyond it.
fn handle_rect(monitor: &Monitor, side: Side, tab: Logical) -> Rect {
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
    tab_rect(monitor, edge, side, tab)
}

/// The outer pixel column of the monitor.
fn edge_x(monitor: &Monitor, side: Side) -> i32 {
    match side {
        Side::Right => monitor.bounds.right - 1,
        Side::Left => monitor.bounds.left,
    }
}

/// A centered activation stretch, clipped to the work area (never the taskbar).
pub fn activation_stretch(monitor: &Monitor, tab: Logical, percent: u32) -> std::ops::Range<i32> {
    let height = (i64::from(Shape::new(tab, monitor.dpi).height) * i64::from(percent) / 100)
        .min(i64::from(monitor.work.bottom - monitor.work.top)) as i32;
    let top = monitor.work.top + (monitor.work.bottom - monitor.work.top - height) / 2;
    top..top + height
}

/// Only the edge column in the activation stretch opens the panel; the tab counts as part of the
/// open panel `panel`. Points outside the monitor never open or hold the panel.
pub fn cursor_zone(
    cursor: Point,
    monitor: &Monitor,
    panel: Rect,
    side: Side,
    tuning: Tuning,
) -> CursorZone {
    let stretch = activation_stretch(monitor, tuning.tab, tuning.activation);
    if !monitor.bounds.contains(cursor) {
        CursorZone::Outside
    } else if cursor.x == edge_x(monitor, side) && stretch.contains(&cursor.y) {
        CursorZone::Edge
    } else if panel.contains(cursor) || tab_rect(monitor, panel, side, tuning.tab).contains(cursor)
    {
        CursorZone::Inside
    } else {
        CursorZone::Outside
    }
}

/// Whether the cursor is in the closed panel's reveal area: within `percent` of the tab height of
/// the parked tab, stretched along the edge to cover the activation stretch when that is longer, on
/// this monitor. The area is a capsule, a circle drawn around the tab's outline rather than its
/// center, so the handle always shows before the edge can open the panel.
pub fn near(cursor: Point, monitor: &Monitor, side: Side, tuning: Tuning) -> bool {
    let handle = handle_rect(monitor, side, tuning.tab);
    let stretch = activation_stretch(monitor, tuning.tab, tuning.activation);
    let (top, bottom) = (
        handle.top.min(stretch.start),
        handle.bottom.max(stretch.end),
    );
    let reach =
        i64::from(Shape::new(tuning.tab, monitor.dpi).height) * i64::from(tuning.reveal) / 100;
    // Distance to the nearest pixel of the tab, 0 inside it.
    let gap =
        |value: i32, start: i32, end: i32| i64::from((start - value).max(value - (end - 1)).max(0));
    let (dx, dy) = (
        gap(cursor.x, handle.left, handle.right),
        gap(cursor.y, top, bottom),
    );
    monitor.bounds.contains(cursor) && dx * dx + dy * dy <= reach * reach
}

/// Whether the cursor, moving from `from` to `to` in `elapsed`, heads for the edge on `side` at
/// `speed` logical pixels per second or faster: a rush, which opens the panel without waiting.
pub fn rushing(
    from: Point,
    to: Point,
    elapsed: Duration,
    monitor: &Monitor,
    side: Side,
    speed: u32,
) -> bool {
    let toward = match side {
        Side::Right => to.x - from.x,
        Side::Left => from.x - to.x,
    };
    // toward / elapsed >= speed * dpi / BASE_DPI, kept in integers.
    toward > 0
        && toward as u128 * 1_000_000 * u128::from(BASE_DPI)
            >= u128::from(speed) * u128::from(monitor.dpi) * elapsed.as_micros()
}

/// Whether the closed panel's handle shows, from one cursor poll to the next.
#[derive(Debug, Clone, Copy, Default)]
pub struct Reveal {
    shown: bool,
    /// The handle hid with the panel and stays hidden until the cursor leaves the reveal area.
    held: bool,
}

impl Reveal {
    /// The panel starts hiding: the handle goes with it, even with the cursor in the reveal area,
    /// where showing it again would undo the close the user just made.
    pub fn hold(&mut self) {
        self.shown = false;
        self.held = true;
    }

    /// The handle shows in the reveal area, except after the panel hid until the cursor leaves it.
    /// A rush holds back a hidden handle, so it never slides in just as the panel opens over it;
    /// it never hides a shown one.
    pub fn step(&mut self, near: bool, rushing: bool) -> bool {
        self.held &= near;
        self.shown = near && !self.held && (self.shown || !rushing);
        self.shown
    }
}

/// How the handle behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Hidden,
    /// Shown, letting clicks through to the scrollbars and buttons below it.
    Through,
    /// The resting open panel's tab: a click closes the panel.
    Tab,
}

/// The handle shows while the panel is open or opening, and otherwise only while revealed: closing,
/// it leaves with the panel. Only the tab of the open panel at rest takes clicks: a tab sliding
/// under the cursor, or the closed panel's handle, never does.
pub fn presence(open: bool, sliding: bool, revealed: bool) -> Presence {
    if !(open || revealed) {
        Presence::Hidden
    } else if open && !sliding {
        Presence::Tab
    } else {
        Presence::Through
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
        cursor_zone(Point { x, y }, &MONITOR, open_panel(side), side, TUNING)
    }

    /// Catches the whole column opening the panel again, as when reaching for a scrollbar at the
    /// edge, or the stretch drifting away from the drawn handle.
    #[test]
    fn only_the_edge_beside_the_handle_opens() {
        // The handle spans 930..1158; the 300% stretch spans 702..1386.
        for (y, opens) in [
            (0, false),
            (701, false),
            (702, true),
            (1040, true),
            (1385, true),
            (1386, false),
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
    fn activation_scales_with_the_tab() {
        for dpi in [96, 120, 144, 192] {
            let monitor = Monitor { dpi, ..MONITOR };
            for percent in [100, 200, 300, 500] {
                let stretch = activation_stretch(&monitor, LOGICAL, percent);
                assert_eq!(
                    stretch.end - stretch.start,
                    Shape::new(LOGICAL, dpi).height * percent as i32 / 100
                );
                // Centered, give or take the pixel an odd difference rounds away.
                let (above, below) = (
                    stretch.start - monitor.work.top,
                    monitor.work.bottom - stretch.end,
                );
                assert!((above - below).abs() <= 1, "above={above} below={below}");
            }
        }
    }

    /// Catches the panel opening from an edge stretch where the handle never showed, even with a
    /// stretch reaching further along the edge than the reveal radius.
    #[test]
    fn the_handle_shows_wherever_the_edge_opens() {
        let tuning = Tuning {
            activation: 900,
            reveal: 10,
            ..TUNING
        };
        let stretch = activation_stretch(&MONITOR, LOGICAL, tuning.activation);
        for y in [stretch.start, stretch.end - 1] {
            for side in [Side::Right, Side::Left] {
                let x = edge_x(&MONITOR, side);
                assert!(near(Point { x, y }, &MONITOR, side, tuning), "y={y}");
            }
        }
    }

    #[test]
    fn activation_is_clipped_to_the_work_area() {
        let monitor = Monitor {
            work: Rect {
                top: 100,
                bottom: 400,
                ..MONITOR.work
            },
            ..MONITOR
        };
        assert_eq!(activation_stretch(&monitor, LOGICAL, 300), 100..400);
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
            let tab = tab_rect(&MONITOR, panel, side, LOGICAL);
            assert_eq!(tab, handle_rect(&MONITOR, side, LOGICAL));
            assert!(MONITOR.bounds.contains(Point {
                x: edge_x(&MONITOR, side),
                y: tab.top
            }));
            assert_eq!((tab.right - tab.left, tab.bottom - tab.top), (30, 228));
        }
    }

    /// Catches the reveal area losing its proportion to the tab, or showing the handle beyond it.
    #[test]
    fn the_reveal_area_reaches_its_radius_around_the_tab() {
        // At 100% activation the stretch is the tab itself, 3810..3840 × 930..1158 on the right;
        // a 100% radius reaches 228 pixels beyond it.
        let tuning = Tuning {
            activation: 100,
            reveal: 100,
            ..TUNING
        };
        let at = |x, y, side| near(Point { x, y }, &MONITOR, side, tuning);
        for (x, y, inside) in [
            (3839, 1040, true),
            (3582, 1040, true),
            (3581, 1040, false),
            (3839, 702, true),
            (3839, 701, false),
            (3839, 1385, true),
            (3839, 1386, false),
        ] {
            assert_eq!(at(x, y, Side::Right), inside, "({x}, {y})");
            assert_eq!(at(3839 - x, y, Side::Left), inside, "({x}, {y})");
        }
        // Off the panel's monitor.
        assert!(!at(3840, 1040, Side::Right));
        assert!(!at(-1, 1040, Side::Left));
    }

    /// Catches the area turning back into a rectangle, whose corners reach further diagonally.
    #[test]
    fn the_reveal_area_rounds_off_beyond_the_tab_corners() {
        let tuning = Tuning {
            activation: 100,
            reveal: 100,
            ..TUNING
        };
        // 161 pixels both ways from the tab's top inner corner is within 228; 162 is not.
        assert!(near(
            Point { x: 3649, y: 769 },
            &MONITOR,
            Side::Right,
            tuning
        ));
        assert!(!near(
            Point { x: 3648, y: 768 },
            &MONITOR,
            Side::Right,
            tuning
        ));
    }

    #[test]
    fn the_reveal_radius_scales_with_the_tab() {
        for dpi in [96, 120, 144, 192] {
            let monitor = Monitor { dpi, ..MONITOR };
            let handle = handle_rect(&monitor, Side::Right, LOGICAL);
            let reach = Shape::new(LOGICAL, dpi).height * 4;
            let y = (handle.top + handle.bottom) / 2;
            let at = |x| near(Point { x, y }, &monitor, Side::Right, TUNING);
            assert!(at(handle.left - reach), "dpi={dpi}");
            assert!(!at(handle.left - reach - 1), "dpi={dpi}");
        }
    }

    /// The rule's threshold in these cases, apart from the tuned default.
    const SPEED: u32 = 3000;

    /// 300 logical pixels in one 50 ms poll is 6000 per second: a flick.
    #[test]
    fn a_flick_toward_the_edge_rushes() {
        let poll = Duration::from_millis(50);
        // At 150% the 300 logical pixels are 450 physical.
        let rush = |from, to, side| {
            rushing(
                Point { x: from, y: 1040 },
                Point { x: to, y: 900 },
                poll,
                &MONITOR,
                side,
                SPEED,
            )
        };
        assert!(rush(3389, 3839, Side::Right));
        assert!(rush(450, 0, Side::Left));
    }

    fn approach(dpi: u32, distance: i32) -> bool {
        let monitor = Monitor { dpi, ..MONITOR };
        let to = Point { x: 3839, y: 1040 };
        let from = Point {
            x: to.x - distance,
            ..to
        };
        let poll = Duration::from_millis(50);
        rushing(from, to, poll, &monitor, Side::Right, SPEED)
    }

    /// Catches the threshold drifting with the monitor's scale: 3000 logical pixels per second is
    /// 150 physical pixels per poll at 100%, 225 at 150% and 300 at 200%.
    #[test]
    fn the_rush_speed_counts_logical_pixels() {
        assert!(approach(96, 150));
        assert!(!approach(96, 149));
        assert!(approach(144, 225));
        assert!(!approach(144, 224));
        assert!(!approach(192, 225));
    }

    #[test]
    fn moving_away_from_the_edge_never_rushes() {
        let (edge, inner) = (Point { x: 3839, y: 1040 }, Point { x: 3000, y: 1040 });
        let instant = Duration::ZERO;
        assert!(!rushing(edge, inner, instant, &MONITOR, Side::Right, 1));
        assert!(!rushing(edge, edge, instant, &MONITOR, Side::Right, 1));
        assert!(!rushing(inner, edge, instant, &MONITOR, Side::Left, 1));
    }

    /// Catches the handle sliding in just as a rushed panel opens over it.
    #[test]
    fn a_rush_holds_back_a_hidden_handle() {
        let mut reveal = Reveal::default();
        assert!(!reveal.step(true, true));
        // It shows once the cursor slows down.
        assert!(reveal.step(true, false));
    }

    /// Catches a shown handle flickering out when the cursor speeds up toward it.
    #[test]
    fn a_rush_never_hides_a_shown_handle() {
        let mut reveal = Reveal::default();
        assert!(reveal.step(true, false));
        assert!(reveal.step(true, true));
    }

    /// Catches the handle staying on the edge, or coming back, right after the panel hid.
    #[test]
    fn the_handle_hides_with_the_panel_until_the_cursor_leaves() {
        let mut reveal = Reveal::default();
        assert!(reveal.step(true, false));
        reveal.hold();
        assert!(!reveal.step(true, false));
        assert!(!reveal.step(false, false));
        assert!(reveal.step(true, false));
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
            let handle = handle_rect(&monitor, Side::Right, LOGICAL);
            let (width, height) = size;
            assert_eq!(handle.right, 3840);
            assert_eq!(handle.right - handle.left, width);
            assert_eq!(handle.bottom - handle.top, height);
            assert_eq!(handle.top - 100, 2088 - handle.bottom);
        }
    }

    /// Catches the closed panel's handle, or a tab sliding under the cursor, taking a click meant
    /// for a scrollbar or a maximized window's buttons, and the handle lingering after a close.
    #[test]
    fn only_the_resting_open_tab_takes_clicks() {
        use Presence::{Hidden, Tab, Through};
        // (open, sliding, revealed)
        for (state, expected) in [
            ((false, false, false), Hidden),
            ((false, false, true), Through),
            ((false, true, false), Hidden),
            ((false, true, true), Through),
            ((true, true, false), Through),
            ((true, true, true), Through),
            ((true, false, false), Tab),
            ((true, false, true), Tab),
        ] {
            let (open, sliding, revealed) = state;
            assert_eq!(presence(open, sliding, revealed), expected, "{state:?}");
        }
    }

    // Catches the widened window swallowing clicks beside the tab, or clipping its outline.
    #[test]
    fn the_window_region_hugs_the_tab() {
        for side in [Side::Right, Side::Left] {
            let panel = open_panel(side);
            let outline = Outline::new(&MONITOR, panel, side, LOGICAL);
            let (depth, width) = (outline.shape.depth, outline.width);
            let rects = outline.rects();
            let row = |y: i32| {
                let rect = rects
                    .iter()
                    .find(|r| (r.top..r.bottom).contains(&y))
                    .unwrap();
                (rect.left, rect.right)
            };
            // Away from the tab only the panel; along its outer side the whole depth.
            let (panel_only, full) = match side {
                Side::Right => ((depth, depth + width), (0, depth + width)),
                Side::Left => ((0, width), (0, width + depth)),
            };
            assert_eq!(row(0), panel_only);
            assert_eq!(row(outline.top - 2), panel_only);
            assert_eq!(row(outline.top + outline.shape.height / 2), full);
            // Halfway down a curve the boundary is halfway out, give or take the slack row.
            let middle = row(outline.top + outline.shape.curve as i32 / 2);
            let reach = match side {
                Side::Right => depth - middle.0,
                Side::Left => middle.1 - width,
            };
            assert!(
                (depth / 2 - 2..=depth / 2 + 6).contains(&reach),
                "{side:?} {reach}"
            );
            assert_eq!(rects.last().unwrap().bottom, outline.height);
        }
    }
}
