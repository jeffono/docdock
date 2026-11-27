//! Pure rules of floating windows: what `.docdock/floating.json` holds, where a window may sit on
//! the monitors there are now, and when a click-through window shows its controls. `floating.rs`
//! runs the effects. Floating images (M16) are the first kind; post-its (M17), notes shown read-only,
//! the second.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dock::Side;
use crate::platform::{Monitor, Point, Rect, SizingEdge, scale_dpi};

/// The most floating windows open at once.
pub const MAX_FLOATING: usize = 8;
/// The only version of `floating.json` this build reads or writes.
pub const VERSION: u64 = 1;
/// The faintest a window gets, so it never disappears altogether.
pub const MIN_OPACITY: f64 = 0.15;
/// Logical pixels of a window that must stay on a monitor's work area for it to stay where it was.
const MIN_VISIBLE: u32 = 48;
/// The smallest side of a window, in logical pixels: room for its controls.
pub const MIN_SIDE: u32 = 96;
/// How far each window opened after another moves down and away from the panel, in logical
/// pixels.
const CASCADE: u32 = 24;
/// Transparent gutter around the image on every side, in logical pixels, where the hover shadow
/// shows and the resize strips sit: `--space-2` in `FloatWindow.svelte`.
pub const SHADOW_PAD: u32 = 8;
/// A post-it's title bar height, in logical pixels, all a collapsed post-it shows: must match the
/// bar's CSS height in the post-it window, as `SHADOW_PAD` matches `--space-2`.
pub const TITLE_HEIGHT: u32 = 28;
/// The most of a monitor's work area an image grows to by zooming, in percent of its width and
/// height: the side preview's share of the room beside the panel.
const MAX_WIDTH: u32 = 72;
const MAX_HEIGHT: u32 = 76;
const LABEL_PREFIX: &str = "float-";
/// The size a post-it opens at, in logical pixels, without its gutter.
pub const NOTE_SIZE: (u32, u32) = (300, 320);
/// How small or large a post-it's text gets against the panel's, in whole percents.
pub const MIN_TEXT_SCALE: f64 = 0.5;
pub const MAX_TEXT_SCALE: f64 = 3.0;

/// The window label for the `n`th floating window of this run, unrelated to saved ids.
pub fn label(n: u64) -> String {
    format!("{LABEL_PREFIX}{n}")
}

/// The `n` of a label `label(n)` gave, and nothing else: no sign, leading zero or other text.
pub fn parse_label(label: &str) -> Option<u64> {
    let digits = label.strip_prefix(LABEL_PREFIX)?;
    let canonical = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'));
    canonical.then(|| digits.parse().ok()).flatten()
}

/// Opacity within `MIN_OPACITY..=1`, at whole percents; anything that is not a number is opaque.
pub fn clamp_opacity(value: f64) -> f64 {
    if !value.is_finite() {
        return 1.0;
    }
    (value.clamp(MIN_OPACITY, 1.0) * 100.0).round() / 100.0
}

/// A post-it's text size within `MIN_TEXT_SCALE..=MAX_TEXT_SCALE`, at whole percents; anything that
/// is not a number is the panel's own size.
pub fn clamp_text_scale(value: f64) -> f64 {
    if !value.is_finite() {
        return 1.0;
    }
    (value.clamp(MIN_TEXT_SCALE, MAX_TEXT_SCALE) * 100.0).round() / 100.0
}

/// A click-through window lets clicks reach it while Alt is held over it, and keeps them through a
/// drag already started with Alt, until the button is released.
pub fn reveal_step(revealed: bool, alt_held: bool, cursor_over: bool, button_down: bool) -> bool {
    (alt_held && cursor_over) || (revealed && button_down)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Image,
    /// A note shown read-only: a post-it.
    Note,
}

/// One saved floating window. Coordinates are physical pixels on the virtual screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// 32 lowercase hexadecimal digits, stable across runs.
    pub id: String,
    pub kind: Kind,
    /// Relative to the notes folder, with `/`.
    pub path: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub opacity: f64,
    /// A post-it's text size against the panel's; absent for images and at the panel's own size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_scale: Option<f64>,
    /// A post-it shows only its title bar; `height` stays its expanded height. Absent when
    /// expanded, so older builds read the same file, and open it expanded.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
}

impl Entry {
    pub fn rect(&self) -> Rect {
        Rect {
            left: self.x,
            top: self.y,
            right: self.x.saturating_add(self.width),
            bottom: self.y.saturating_add(self.height),
        }
    }

    pub fn set_rect(&mut self, rect: Rect) {
        (self.x, self.y) = (rect.left, rect.top);
        (self.width, self.height) = (rect.right - rect.left, rect.bottom - rect.top);
    }

    fn valid(&self) -> bool {
        valid_id(&self.id) && !self.path.is_empty() && self.width > 0 && self.height > 0
    }
}

pub fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// An entry this build understands, or one it keeps as it was, such as a later kind.
#[derive(Debug, Clone, PartialEq)]
enum Slot {
    Known(Entry),
    Other(Value),
}

/// The saved floating windows, in file order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Store {
    slots: Vec<Slot>,
}

/// Why a file gives no store.
#[derive(Debug, PartialEq)]
pub enum Unreadable {
    /// Not the file's shape: read as empty.
    Corrupt,
    /// Written by another version of DocDock: neither opened nor overwritten.
    Version(u64),
}

#[derive(Serialize)]
struct File {
    version: u64,
    entries: Vec<Value>,
}

/// Reads `floating.json`. Entries of an unknown kind or shape are kept as they are.
pub fn parse(bytes: &[u8]) -> Result<Store, Unreadable> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| Unreadable::Corrupt)?;
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .ok_or(Unreadable::Corrupt)?;
    if version != VERSION {
        return Err(Unreadable::Version(version));
    }
    let Some(entries) = value.get("entries").and_then(Value::as_array) else {
        return Err(Unreadable::Corrupt);
    };
    let slots = entries
        .iter()
        .map(
            |entry| match serde_json::from_value::<Entry>(entry.clone()) {
                Ok(mut known) if known.valid() => {
                    known.opacity = clamp_opacity(known.opacity);
                    known.text_scale = known.text_scale.map(clamp_text_scale);
                    Slot::Known(known)
                }
                _ => Slot::Other(entry.clone()),
            },
        )
        .collect();
    Ok(Store { slots })
}

/// Every `path` an entry names, whatever its kind or the file's version, for the images that count
/// as in use. A file that does not read names none.
pub fn paths(bytes: &[u8]) -> Vec<String> {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return Vec::new();
    };
    value
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("path")?.as_str().map(str::to_owned))
        .collect()
}

impl Store {
    pub fn to_bytes(&self) -> Vec<u8> {
        let entries = self
            .slots
            .iter()
            .map(|slot| match slot {
                Slot::Known(entry) => serde_json::to_value(entry).unwrap_or(Value::Null),
                Slot::Other(value) => value.clone(),
            })
            .collect();
        let file = File {
            version: VERSION,
            entries,
        };
        serde_json::to_vec_pretty(&file).unwrap_or_default()
    }

    /// The entries this build opens, in file order.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.slots.iter().filter_map(|slot| match slot {
            Slot::Known(entry) => Some(entry),
            Slot::Other(_) => None,
        })
    }

    fn entry_mut(&mut self, id: &str) -> Option<&mut Entry> {
        self.slots.iter_mut().find_map(|slot| match slot {
            Slot::Known(entry) if entry.id == id => Some(entry),
            _ => None,
        })
    }

    pub fn add(&mut self, entry: Entry) {
        self.slots.push(Slot::Known(entry));
    }

    /// Moves or fades the window `id`, sizes its text or collapses it (post-its only, `None` for
    /// images), if it is still saved: a window closed meanwhile stays gone. `rect` is a collapsed
    /// post-it's expanded rectangle (`saved_rect`).
    pub fn update(
        &mut self,
        id: &str,
        rect: Rect,
        opacity: f64,
        text_scale: Option<f64>,
        collapsed: Option<bool>,
    ) {
        if let Some(entry) = self.entry_mut(id) {
            entry.set_rect(rect);
            entry.opacity = clamp_opacity(opacity);
            entry.text_scale = text_scale
                .map(clamp_text_scale)
                .filter(|&scale| scale != 1.0);
            if let Some(collapsed) = collapsed {
                entry.collapsed = collapsed;
            }
        }
    }

    /// The post-it `id` now shows the note at `path`, as after its note was renamed; images and
    /// windows closed meanwhile stay as they are.
    pub fn set_note_path(&mut self, id: &str, path: &str) {
        if let Some(entry) = self.entry_mut(id)
            && entry.kind == Kind::Note
        {
            entry.path = path.to_owned();
        }
    }

    pub fn remove(&mut self, ids: &[&str]) {
        self.slots.retain(|slot| match slot {
            Slot::Known(entry) => !ids.contains(&entry.id.as_str()),
            Slot::Other(_) => true,
        });
    }
}

fn intersection(a: Rect, b: Rect) -> Option<Rect> {
    let rect = Rect {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    (rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
}

fn area(rect: Option<Rect>) -> i64 {
    rect.map_or(0, |r| {
        i64::from(r.right - r.left) * i64::from(r.bottom - r.top)
    })
}

/// Squared distance from `rect`'s center to `work`, 0 inside it.
fn distance(rect: Rect, work: Rect) -> i64 {
    let x = (i64::from(rect.left) + i64::from(rect.right)) / 2;
    let y = (i64::from(rect.top) + i64::from(rect.bottom)) / 2;
    let dx = (i64::from(work.left) - x)
        .max(x - i64::from(work.right))
        .max(0);
    let dy = (i64::from(work.top) - y)
        .max(y - i64::from(work.bottom))
        .max(0);
    dx * dx + dy * dy
}

/// `rect` resized within `MIN_SIDE` and `work`'s size, keeping its top left corner.
fn sized(rect: Rect, work: Rect, dpi: u32) -> Rect {
    let min = scale_dpi(dpi, MIN_SIDE);
    let fit = |length: i32, room: i32| length.max(min).min(room.max(1));
    let width = fit(rect.right.saturating_sub(rect.left), work.right - work.left);
    let height = fit(rect.bottom.saturating_sub(rect.top), work.bottom - work.top);
    Rect {
        right: rect.left.saturating_add(width),
        bottom: rect.top.saturating_add(height),
        ..rect
    }
}

/// Where a saved window goes on the monitors there are now: first sized to the work area of the
/// monitor that shows most of it (never larger, nor smaller than `MIN_SIDE`), then left where it
/// was while at least `MIN_VISIBLE` logical pixels of it each way stay on that work area;
/// otherwise sized to and moved fully inside the nearest monitor's work area. Without monitors, as
/// during a display change, it stays as it is.
pub fn fit_to_monitors(rect: Rect, monitors: &[Monitor]) -> Rect {
    let most = monitors
        .iter()
        .max_by_key(|monitor| area(intersection(rect, monitor.work)));
    let Some(monitor) = most else {
        return rect;
    };
    // Shrinking keeps the top left corner, which can take what showed off screen: the window is
    // judged as it will be.
    let resized = sized(rect, monitor.work, monitor.dpi);
    if let Some(shown) = intersection(resized, monitor.work) {
        let min = scale_dpi(monitor.dpi, MIN_VISIBLE);
        if shown.right - shown.left >= min && shown.bottom - shown.top >= min {
            return resized;
        }
    }
    let nearest = monitors
        .iter()
        .min_by_key(|monitor| distance(resized, monitor.work))
        .unwrap_or(monitor);
    let work = nearest.work;
    let rect = sized(resized, work, nearest.dpi);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let left = rect.left.clamp(work.left, work.right - width);
    let top = rect.top.clamp(work.top, work.bottom - height);
    Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// Where the panel thread opens a floating image: the image's box, its monitor's DPI, the panel's
/// side and the room beside the panel on that monitor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub image: Rect,
    pub dpi: u32,
    pub side: Side,
    pub free: Rect,
}

/// `rect` moved down and away from the panel on `side` for the `index`th window opened beside the
/// same place, so a new window never hides one already there; kept within `free`, the room beside
/// the panel, as far as it fits.
pub fn cascade(rect: Rect, index: usize, dpi: u32, side: Side, free: Rect) -> Rect {
    let step = scale_dpi(dpi, CASCADE).saturating_mul(index as i32);
    let across = match side {
        Side::Right => -step,
        Side::Left => step,
    };
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let within = |start: i32, low: i32, high: i32, length: i32| {
        if length <= high - low {
            start.clamp(low, high - length)
        } else {
            start
        }
    };
    let left = within(
        rect.left.saturating_add(across),
        free.left,
        free.right,
        width,
    );
    let top = within(rect.top.saturating_add(step), free.top, free.bottom, height);
    Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// `rect` grown by `by` on every side: a window around its image.
pub fn outset(rect: Rect, by: i32) -> Rect {
    Rect {
        left: rect.left - by,
        top: rect.top - by,
        right: rect.right + by,
        bottom: rect.bottom + by,
    }
}

/// `rect` shrunk by `by` on every side: the image within its window.
pub fn inset(rect: Rect, by: i32) -> Rect {
    outset(rect, -by)
}

/// Width over height of `rect`, 1 for an empty one.
pub fn aspect_of(rect: Rect) -> f64 {
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width > 0 && height > 0 {
        f64::from(width) / f64::from(height)
    } else {
        1.0
    }
}

/// The narrowest an image of `aspect` gets: its shorter side `min` pixels.
fn min_width(aspect: f64, min: i32) -> f64 {
    f64::from(min) * aspect.max(1.0)
}

/// The widest image of `aspect` within `width` by `height` pixels.
fn fit_width(width: i32, height: i32, aspect: f64) -> f64 {
    f64::from(width.max(0)).min(f64::from(height.max(0)) * aspect)
}

/// A window for its image of `aspect`, `width` pixels wide as far as `max` (the image's largest
/// width and height) allows, with `pad` around it, its corner at `left`, `top`. Both sides come
/// from the exact proportions before rounding, so a skinny image never rounds past `max`.
fn window_at(left: i32, top: i32, width: f64, aspect: f64, pad: i32, max: (i32, i32)) -> Rect {
    let (max_width, max_height) = (max.0.max(1), max.1.max(1));
    let width = width.min(f64::from(max_width));
    let (width, height) = match width / aspect {
        height if height > f64::from(max_height) => {
            (f64::from(max_height) * aspect, f64::from(max_height))
        }
        height => (width, height),
    };
    let width = (width.round() as i32).clamp(1, max_width);
    let height = (height.round() as i32).clamp(1, max_height);
    Rect {
        left,
        top,
        right: left + width + 2 * pad,
        bottom: top + height + 2 * pad,
    }
}

/// The window rectangle `rect` with its image, `pad` inside each side, reshaped to `aspect`:
/// the largest such image within it, keeping its top left corner.
pub fn contain(rect: Rect, aspect: f64, pad: i32) -> Rect {
    let image = inset(rect, pad);
    let room = (image.right - image.left, image.bottom - image.top);
    let width = fit_width(room.0, room.1, aspect);
    window_at(rect.left, rect.top, width, aspect, pad, room)
}

/// What a resize from `edge` may make of the window `rect`, whose image of `aspect` sits `pad`
/// inside it: the image keeps its proportions, the edge or corner opposite the one dragged stays,
/// the image's shorter side never falls under `min` and the window never grows past `work`, which
/// wins over `min` for a very long image. A side edge sets the image's width or height; a corner,
/// whichever grew more.
pub fn constrain_sizing(
    rect: Rect,
    edge: SizingEdge,
    aspect: f64,
    pad: i32,
    min: i32,
    work: Rect,
) -> Rect {
    use SizingEdge::*;
    let image = inset(rect, pad);
    let (width, height) = (
        f64::from(image.right - image.left),
        f64::from(image.bottom - image.top),
    );
    let wanted = match edge {
        Left | Right => width,
        Top | Bottom => height * aspect,
        _ => width.max(height * aspect),
    };
    let leftward = matches!(edge, Left | TopLeft | BottomLeft);
    let upward = matches!(edge, Top | TopLeft | TopRight);
    let room_width = if leftward {
        rect.right - work.left
    } else {
        work.right - rect.left
    };
    let room_height = if upward {
        rect.bottom - work.top
    } else {
        work.bottom - rect.top
    };
    let room = (room_width - 2 * pad, room_height - 2 * pad);
    let widest = fit_width(room.0, room.1, aspect);
    let width = wanted.max(min_width(aspect, min)).min(widest);
    let sized = window_at(0, 0, width, aspect, pad, room);
    let (window_width, window_height) = (sized.right, sized.bottom);
    let left = if leftward {
        rect.right - window_width
    } else {
        rect.left
    };
    let top = if upward {
        rect.bottom - window_height
    } else {
        rect.top
    };
    Rect {
        left,
        top,
        right: left + window_width,
        bottom: top + window_height,
    }
}

/// What a resize from `edge` may make of a post-it's window `rect`: any proportions, the edge or
/// corner opposite the one dragged staying, each side of the post-it (`pad` inside the window) at
/// least `min`, and the window within `work`, which wins over `min`. A collapsed post-it keeps its
/// window `collapsed` pixels tall (`collapsed_height`) whatever is dragged; only its width changes.
pub fn constrain_free(
    rect: Rect,
    edge: SizingEdge,
    pad: i32,
    min: i32,
    work: Rect,
    collapsed: Option<i32>,
) -> Rect {
    use SizingEdge::*;
    let leftward = matches!(edge, Left | TopLeft | BottomLeft);
    let upward = matches!(edge, Top | TopLeft | TopRight);
    let mut rect = rect;
    let least = min + 2 * pad;
    if leftward {
        rect.left = rect.left.min(rect.right - least).max(work.left);
    } else {
        rect.right = rect.right.max(rect.left + least).min(work.right);
    }
    match (collapsed, upward) {
        (Some(height), true) => rect.top = rect.bottom - height,
        (Some(height), false) => rect.bottom = rect.top + height,
        (None, true) => rect.top = rect.top.min(rect.bottom - least).max(work.top),
        (None, false) => rect.bottom = rect.bottom.max(rect.top + least).min(work.bottom),
    }
    rect
}

/// The height of a collapsed post-it's window on a monitor of `dpi`: its title bar, with the
/// gutter `pad` above and below.
pub fn collapsed_height(dpi: u32, pad: i32) -> i32 {
    scale_dpi(dpi, TITLE_HEIGHT) + 2 * pad
}

/// The window of the post-it `expanded` collapsed to its title bar: same left, top and width.
pub fn collapsed_rect(expanded: Rect, dpi: u32, pad: i32) -> Rect {
    Rect {
        bottom: expanded.top + collapsed_height(dpi, pad),
        ..expanded
    }
}

/// Where the saved expanded rectangle of a collapsed post-it goes on the monitors there are now,
/// judged by its title bar, all that shows: on the monitor that shows most of the bar, or the
/// nearest, sized as `fit_to_monitors` would, and moved only as far as the whole bar needs to lie
/// on the work area. The expanded rectangle may reach past it, as expanding moves it back
/// (`expand_rect`).
pub fn fit_collapsed(expanded: Rect, monitors: &[Monitor]) -> Rect {
    let bar_height =
        |monitor: &Monitor| collapsed_height(monitor.dpi, scale_dpi(monitor.dpi, SHADOW_PAD));
    let bar = |monitor: &Monitor| Rect {
        bottom: expanded.top.saturating_add(bar_height(monitor)),
        ..expanded
    };
    let most = monitors
        .iter()
        .map(|monitor| (monitor, area(intersection(bar(monitor), monitor.work))))
        .filter(|(_, shown)| *shown > 0)
        .max_by_key(|(_, shown)| *shown)
        .map(|(monitor, _)| monitor);
    let nearest = || {
        monitors
            .iter()
            .min_by_key(|monitor| distance(bar(monitor), monitor.work))
    };
    let Some(monitor) = most.or_else(nearest) else {
        return expanded;
    };
    let work = monitor.work;
    let fitted = sized(expanded, work, monitor.dpi);
    let (width, height) = (fitted.right - fitted.left, fitted.bottom - fitted.top);
    let left = fitted.left.min(work.right - width).max(work.left);
    let top = fitted
        .top
        .min(work.bottom - bar_height(monitor))
        .max(work.top);
    Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// The window of the collapsed post-it `bar` expanded to `expanded_height` pixels: growing down
/// from the bar, moved up as far as needed to stay within `work`, and no taller than it.
pub fn expand_rect(bar: Rect, expanded_height: i32, work: Rect) -> Rect {
    let height = expanded_height.clamp(1, (work.bottom - work.top).max(1));
    let top = bar.top.min(work.bottom - height).max(work.top);
    Rect {
        top,
        bottom: top + height,
        ..bar
    }
}

/// The rectangle saved for a window at `window`: a collapsed post-it's is its expanded one,
/// `expanded_height` pixels tall from the bar's top.
pub fn saved_rect(window: Rect, collapsed: bool, expanded_height: i32) -> Rect {
    if collapsed {
        Rect {
            bottom: window.top + expanded_height,
            ..window
        }
    } else {
        window
    }
}

/// The largest an image may grow to by zooming on a monitor with this work area.
pub fn max_image_size(work: Rect) -> (i32, i32) {
    let percent = |length: i32, share: u32| length * share as i32 / 100;
    (
        percent(work.right - work.left, MAX_WIDTH),
        percent(work.bottom - work.top, MAX_HEIGHT),
    )
}

/// The window `window` zoomed so its image of `aspect`, `pad` inside it, is `width` pixels wide:
/// between `min` for its shorter side and `max_image_size`, around the same center, moved into
/// `work`. Also answers whether the image reached its largest size.
pub fn zoom_rect(
    window: Rect,
    width: f64,
    aspect: f64,
    pad: i32,
    min: i32,
    work: Rect,
) -> (Rect, bool) {
    let (max_width, max_height) = max_image_size(work);
    let widest = fit_width(max_width, max_height, aspect);
    let narrowest = min_width(aspect, min).min(widest);
    let width = if width.is_finite() { width } else { widest };
    let width = width.clamp(narrowest, widest);
    let at_max = width >= widest - 0.5;
    let sized = window_at(0, 0, width, aspect, pad, (max_width, max_height));
    let (window_width, window_height) = (sized.right, sized.bottom);
    let center_x = (window.left + window.right) / 2;
    let center_y = (window.top + window.bottom) / 2;
    let place = |center: i32, length: i32, low: i32, high: i32| {
        let start = center - length / 2;
        if length <= high - low {
            start.clamp(low, high - length)
        } else {
            low
        }
    };
    let left = place(center_x, window_width, work.left, work.right);
    let top = place(center_y, window_height, work.top, work.bottom);
    let rect = Rect {
        left,
        top,
        right: left + window_width,
        bottom: top + window_height,
    };
    (rect, at_max)
}

/// The monitor showing most of `rect`, else the nearest one.
pub fn monitor_of(rect: Rect, monitors: &[Monitor]) -> Option<Monitor> {
    let most = monitors
        .iter()
        .max_by_key(|monitor| area(intersection(rect, monitor.work)))?;
    if intersection(rect, most.work).is_some() {
        return Some(*most);
    }
    monitors
        .iter()
        .min_by_key(|monitor| distance(rect, monitor.work))
        .copied()
}

/// An image of `natural` pixels in the middle of `work`, at its natural size in CSS pixels but no
/// more than half of `work` each way, for a window that cannot open beside the panel.
pub fn centered(work: Rect, natural: (u32, u32), dpi: u32) -> Rect {
    let (room_width, room_height) = (
        f64::from((work.right - work.left) / 2),
        f64::from((work.bottom - work.top) / 2),
    );
    let css = |pixels: u32| f64::from(pixels.max(1)) * f64::from(dpi) / 96.0;
    let (width, height) = (css(natural.0), css(natural.1));
    let scale = (room_width / width).min(room_height / height).min(1.0);
    let width = ((width * scale).round() as i32).max(1);
    let height = ((height * scale).round() as i32).max(1);
    let left = work.left + (work.right - work.left - width) / 2;
    let top = work.top + (work.bottom - work.top - height) / 2;
    Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// Where the pointer dropped a note outside the panel: cursor position, monitor work area and DPI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropSpot {
    pub cursor: Point,
    pub work: Rect,
    pub dpi: u32,
}

fn place_at_cursor(spot: DropSpot, width: i32, height: i32, pad: i32) -> Rect {
    let work = spot.work;
    let width = width.clamp(1, (work.right - work.left).max(1));
    let height = height.clamp(1, (work.bottom - work.top).max(1));
    let grab_y = scale_dpi(spot.dpi, 24) + pad;
    let left = (spot.cursor.x - width / 2).clamp(work.left, (work.right - width).max(work.left));
    let top = (spot.cursor.y - grab_y).clamp(work.top, (work.bottom - height).max(work.top));
    Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// Window rectangle for a new post-it of logical `size` dropped at `spot`.
pub fn drop_rect(spot: DropSpot, size: (u32, u32), pad: i32) -> Rect {
    let width = scale_dpi(spot.dpi, size.0) + 2 * pad;
    let height = scale_dpi(spot.dpi, size.1) + 2 * pad;
    place_at_cursor(spot, width, height, pad)
}

/// Window rectangle for an existing post-it moved to `spot`, keeping its current size.
pub fn drop_existing_rect(spot: DropSpot, current: Rect, pad: i32) -> Rect {
    place_at_cursor(
        spot,
        current.right - current.left,
        current.bottom - current.top,
        pad,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef";

    fn entry(id: &str, path: &str) -> Entry {
        Entry {
            id: id.into(),
            kind: Kind::Image,
            path: path.into(),
            x: 10,
            y: 20,
            width: 300,
            height: 200,
            opacity: 0.6,
            text_scale: None,
            collapsed: false,
        }
    }

    fn rect(left: i32, top: i32, width: i32, height: i32) -> Rect {
        Rect {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }

    fn monitor(left: i32, width: i32, dpi: u32) -> Monitor {
        let bounds = rect(left, 0, width, 1080);
        Monitor {
            bounds,
            work: Rect {
                bottom: 1040,
                ..bounds
            },
            dpi,
        }
    }

    // Catches a save that loses or reorders what the next run reopens.
    #[test]
    fn a_store_survives_a_round_trip() {
        let mut store = Store::default();
        store.add(entry(ID, "attachments/a.png"));
        store.add(entry(&"f".repeat(32), "sub/attachments/b.webp"));
        assert_eq!(parse(&store.to_bytes()), Ok(store));
    }

    // Catches this build erasing what a later one saved, such as a kind it does not know.
    #[test]
    fn entries_of_an_unknown_kind_or_shape_are_kept_in_place() {
        let note =
            serde_json::json!({ "id": ID, "kind": "sticker", "path": "a.md", "pinned": true });
        let broken = serde_json::json!({ "id": "short", "kind": "image", "path": "x.png" });
        let file = serde_json::json!({
            "version": 1,
            "entries": [note, entry(&"a".repeat(32), "a.png"), broken],
        });
        let mut store = parse(&serde_json::to_vec(&file).unwrap()).unwrap();
        assert_eq!(store.entries().count(), 1);
        store.remove(&[&"a".repeat(32)]);
        store.add(entry(ID, "b.png"));
        let saved: Value = serde_json::from_slice(&store.to_bytes()).unwrap();
        assert_eq!(saved["entries"][0], note);
        assert_eq!(saved["entries"][1], broken);
        assert_eq!(saved["entries"][2]["path"], "b.png");
    }

    // Catches an older build opening or overwriting a file a newer one wrote.
    #[test]
    fn an_unknown_version_is_refused() {
        for version in [0, 2, 99] {
            let file = serde_json::json!({ "version": version, "entries": [] });
            let parsed = parse(&serde_json::to_vec(&file).unwrap());
            assert_eq!(parsed, Err(Unreadable::Version(version)));
        }
    }

    #[test]
    fn a_file_of_another_shape_is_corrupt() {
        for bytes in [
            &b"{"[..],
            b"[]",
            br#"{"entries":[]}"#,
            br#"{"version":"1","entries":[]}"#,
            br#"{"version":1,"entries":{}}"#,
        ] {
            assert_eq!(parse(bytes), Err(Unreadable::Corrupt), "{bytes:?}");
        }
    }

    // Catches a hand-edited or synced opacity making a window invisible or opaque by mistake.
    #[test]
    fn opacity_stays_between_the_minimum_and_opaque() {
        assert_eq!(clamp_opacity(0.0), MIN_OPACITY);
        assert_eq!(clamp_opacity(-3.0), MIN_OPACITY);
        assert_eq!(clamp_opacity(7.0), 1.0);
        assert_eq!(clamp_opacity(0.555), 0.56);
        assert_eq!(clamp_opacity(f64::NAN), 1.0);
        assert_eq!(clamp_opacity(f64::INFINITY), 1.0);
        let mut faded = entry(ID, "a.png");
        faded.opacity = 0.01;
        let file = serde_json::json!({ "version": 1, "entries": [faded] });
        let store = parse(&serde_json::to_vec(&file).unwrap()).unwrap();
        assert_eq!(store.entries().next().unwrap().opacity, MIN_OPACITY);
    }

    // Catches an image some floating window shows going to the trash as unused, whatever kind or
    // version named it.
    #[test]
    fn paths_come_from_every_entry_of_any_version() {
        let file = serde_json::json!({
            "version": 7,
            "entries": [{ "path": "a.png" }, { "kind": "note", "path": "n.md" }, { "id": 1 }],
        });
        assert_eq!(
            paths(&serde_json::to_vec(&file).unwrap()),
            ["a.png", "n.md"]
        );
        assert!(paths(b"not json").is_empty());
    }

    #[test]
    fn updates_reach_only_saved_windows() {
        let mut store = Store::default();
        store.add(entry(ID, "a.png"));
        store.update(ID, rect(-50, 5, 400, 300), 0.3, None, None);
        store.update(&"b".repeat(32), rect(0, 0, 1, 1), 1.0, None, None);
        let saved: Vec<_> = store.entries().cloned().collect();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].rect(), rect(-50, 5, 400, 300));
        assert_eq!(saved[0].opacity, 0.3);
    }

    // Catches a post-it's text size lost on restart, or one saved for every image.
    #[test]
    fn a_text_size_is_saved_only_when_it_differs() {
        let mut store = Store::default();
        store.add(Entry {
            kind: Kind::Note,
            ..entry(ID, "a.md")
        });
        store.update(ID, rect(0, 0, 300, 300), 1.0, Some(1.25), None);
        let saved = parse(&store.to_bytes()).unwrap();
        assert_eq!(saved.entries().next().unwrap().text_scale, Some(1.25));
        store.update(ID, rect(0, 0, 300, 300), 1.0, Some(1.0), None);
        let bytes = store.to_bytes();
        assert!(!String::from_utf8(bytes).unwrap().contains("textScale"));
        assert_eq!(clamp_text_scale(9.0), MAX_TEXT_SCALE);
        assert_eq!(clamp_text_scale(f64::NAN), 1.0);
    }

    fn note(id: &str) -> Entry {
        Entry {
            kind: Kind::Note,
            ..entry(id, "a.md")
        }
    }

    // Catches a file an older build wrote opening its post-its collapsed, or unreadable.
    #[test]
    fn an_entry_without_collapsed_reads_expanded() {
        let mut old = serde_json::to_value(note(ID)).unwrap();
        old.as_object_mut().unwrap().remove("collapsed");
        let file = serde_json::json!({ "version": 1, "entries": [old] });
        let store = parse(&serde_json::to_vec(&file).unwrap()).unwrap();
        assert!(!store.entries().next().unwrap().collapsed);
    }

    // Catches a collapsed post-it reopening expanded, or at its title bar's height once expanded.
    #[test]
    fn a_collapsed_post_it_round_trips_with_its_expanded_height() {
        let mut store = Store::default();
        store.add(note(ID));
        store.update(ID, rect(10, 20, 300, 340), 1.0, None, Some(true));
        let saved = parse(&store.to_bytes()).unwrap();
        let entry = saved.entries().next().unwrap();
        assert!(entry.collapsed);
        assert_eq!(entry.rect(), rect(10, 20, 300, 340));
    }

    // Catches expanded post-its writing a key older builds would see, changing every saved file.
    #[test]
    fn an_expanded_post_it_writes_no_collapsed_key() {
        let mut store = Store::default();
        store.add(note(ID));
        store.update(ID, rect(10, 20, 300, 340), 1.0, None, Some(true));
        store.update(ID, rect(10, 20, 300, 340), 1.0, None, Some(false));
        let bytes = store.to_bytes();
        assert!(!String::from_utf8(bytes).unwrap().contains("collapsed"));
    }

    // Catches a renamed note's new path landing on an image's window, or on another post-it.
    #[test]
    fn only_the_named_post_it_takes_a_new_path() {
        let mut store = Store::default();
        let note = |id: &str, path: &str| Entry {
            kind: Kind::Note,
            ..entry(id, path)
        };
        store.add(note(ID, "Inbox/Todo.md"));
        store.add(note(&"b".repeat(32), "other.md"));
        store.add(entry(&"c".repeat(32), "a.png"));
        store.set_note_path(ID, "Done/Todo.md");
        store.set_note_path(&"c".repeat(32), "x.md");
        let paths: Vec<_> = store.entries().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["Done/Todo.md", "other.md", "a.png"]);
    }

    #[test]
    fn labels_parse_only_as_written() {
        assert_eq!(parse_label(&label(7)), Some(7));
        assert_eq!(parse_label(&label(0)), Some(0));
        for other in [
            "float-",
            "float-07",
            "float-+7",
            "float-7a",
            "float--1",
            "image-preview",
            "main",
            "Float-7",
            "float-99999999999999999999999",
        ] {
            assert_eq!(parse_label(other), None, "{other}");
        }
    }

    // Catches releasing Alt mid-drag dropping the window, or a lingering reveal after the drag.
    #[test]
    fn a_reveal_lasts_while_alt_is_held_over_it_or_a_drag_goes_on() {
        assert!(reveal_step(false, true, true, false));
        assert!(!reveal_step(false, true, false, false));
        assert!(!reveal_step(false, false, true, true));
        assert!(reveal_step(true, false, false, true));
        assert!(!reveal_step(true, false, true, false));
        assert!(reveal_step(true, true, true, false));
    }

    #[test]
    fn a_window_on_a_monitor_stays_where_it_was() {
        let monitors = [monitor(0, 1920, 96), monitor(1920, 1920, 96)];
        let shown = rect(2000, 100, 400, 300);
        assert_eq!(fit_to_monitors(shown, &monitors), shown);
        // Mostly beyond the left edge, but 60 pixels still on screen.
        let edge = rect(-340, 100, 400, 300);
        assert_eq!(fit_to_monitors(edge, &monitors), edge);
    }

    // Catches a window saved on a monitor that is no longer there opening out of reach.
    #[test]
    fn a_window_on_a_monitor_gone_moves_onto_the_nearest() {
        let monitors = [monitor(0, 1920, 96)];
        let fitted = fit_to_monitors(rect(2500, 100, 400, 300), &monitors);
        assert_eq!(fitted, rect(1520, 100, 400, 300));
        let fitted = fit_to_monitors(rect(-900, -700, 400, 300), &monitors);
        assert_eq!(fitted, rect(0, 0, 400, 300));
    }

    #[test]
    fn a_window_barely_on_screen_moves_fully_onto_it() {
        let monitors = [monitor(0, 1920, 96)];
        // 30 pixels on screen, under 48.
        let fitted = fit_to_monitors(rect(1890, 100, 400, 300), &monitors);
        assert_eq!(fitted, rect(1520, 100, 400, 300));
    }

    // Catches the visible minimum counted in physical pixels on a scaled monitor.
    #[test]
    fn the_visible_minimum_scales_with_the_monitor() {
        let monitors = [monitor(0, 3840, 192)];
        // 60 physical pixels are 30 logical, under 48.
        let fitted = fit_to_monitors(rect(3780, 100, 400, 300), &monitors);
        assert_eq!(fitted, rect(3440, 100, 400, 300));
    }

    #[test]
    fn a_window_larger_than_the_work_area_shrinks_to_it() {
        let monitors = [monitor(0, 1280, 96)];
        let fitted = fit_to_monitors(rect(2000, 0, 3000, 2000), &monitors);
        assert_eq!(fitted, rect(0, 0, 1280, 1040));
        let shown = fit_to_monitors(rect(100, 100, 3000, 2000), &monitors);
        assert_eq!(shown, rect(100, 100, 1280, 1040));
    }

    // Regression: a wide window mostly off the left edge was judged visible before shrinking,
    // which then took all of it off screen.
    #[test]
    fn a_window_is_judged_visible_once_shrunk() {
        let monitors = [monitor(0, 1920, 96)];
        let fitted = fit_to_monitors(rect(-2200, 100, 2600, 300), &monitors);
        assert_eq!(fitted, rect(0, 100, 1920, 300));
    }

    #[test]
    fn a_tiny_window_grows_to_the_minimum() {
        let monitors = [monitor(0, 1920, 144)];
        let fitted = fit_to_monitors(rect(100, 100, 10, 10), &monitors);
        assert_eq!(fitted, rect(100, 100, 144, 144));
    }

    // Catches a window spanning two monitors jumping to the one that shows less of it.
    #[test]
    fn the_monitor_showing_most_of_a_window_keeps_it() {
        let monitors = [monitor(0, 1920, 96), monitor(1920, 1920, 192)];
        let spanning = rect(1800, 100, 400, 300);
        assert_eq!(fit_to_monitors(spanning, &monitors), spanning);
    }

    #[test]
    fn without_monitors_a_window_stays_as_it_is() {
        let saved = rect(-5000, -5000, 10, 10);
        assert_eq!(fit_to_monitors(saved, &[]), saved);
    }

    const FREE_RIGHT: Rect = Rect {
        left: 0,
        top: 0,
        right: 1400,
        bottom: 1040,
    };

    // Catches later windows drifting under a panel on the right.
    #[test]
    fn later_windows_cascade_away_from_a_panel_on_the_right() {
        let moved = cascade(rect(1000, 100, 300, 200), 2, 144, Side::Right, FREE_RIGHT);
        assert_eq!(moved, rect(928, 172, 300, 200));
    }

    #[test]
    fn later_windows_cascade_away_from_a_panel_on_the_left() {
        let free = Rect {
            left: 500,
            ..FREE_RIGHT
        };
        let moved = cascade(rect(520, 100, 300, 200), 1, 96, Side::Left, free);
        assert_eq!(moved, rect(544, 124, 300, 200));
    }

    // Catches a cascade pushing a window off the room beside the panel, or under the panel.
    #[test]
    fn a_cascade_stays_beside_the_panel() {
        let moved = cascade(rect(10, 900, 300, 200), 3, 96, Side::Right, FREE_RIGHT);
        assert_eq!(moved, rect(0, 840, 300, 200));
        let free = Rect {
            left: 500,
            ..FREE_RIGHT
        };
        let moved = cascade(rect(1300, 100, 300, 200), 1, 96, Side::Left, free);
        assert_eq!(moved, rect(1100, 124, 300, 200));
    }

    const PAD: i32 = 8;
    const WORK: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    /// A window around an image `width` by `height` at `left`, `top`.
    fn window(left: i32, top: i32, width: i32, height: i32) -> Rect {
        rect(left, top, width + 2 * PAD, height + 2 * PAD)
    }

    fn size(rect: Rect) -> (i32, i32) {
        (rect.right - rect.left, rect.bottom - rect.top)
    }

    // Catches a resize from a side edge leaving the image's proportions or moving the opposite
    // edge, which left the window with transparent margins and the controls off the image.
    #[test]
    fn side_edges_keep_the_proportions_and_the_opposite_edge() {
        use SizingEdge::*;
        let start = window(400, 300, 400, 200);
        // Dragged 200 pixels out on each side in turn.
        let right = constrain_sizing(
            Rect {
                right: start.right + 200,
                ..start
            },
            Right,
            2.0,
            PAD,
            96,
            WORK,
        );
        assert_eq!(right, window(400, 300, 600, 300));
        let left = constrain_sizing(
            Rect {
                left: start.left - 200,
                ..start
            },
            Left,
            2.0,
            PAD,
            96,
            WORK,
        );
        assert_eq!(left, window(200, 300, 600, 300));
        assert_eq!(left.right, start.right);
        let bottom = constrain_sizing(
            Rect {
                bottom: start.bottom + 100,
                ..start
            },
            Bottom,
            2.0,
            PAD,
            96,
            WORK,
        );
        assert_eq!(bottom, window(400, 300, 600, 300));
        let top = constrain_sizing(
            Rect {
                top: start.top - 100,
                ..start
            },
            Top,
            2.0,
            PAD,
            96,
            WORK,
        );
        assert_eq!((top.bottom, top.left), (start.bottom, start.left));
        assert_eq!(size(top), (616, 316));
    }

    #[test]
    fn corners_keep_the_opposite_corner_and_follow_the_larger_pull() {
        use SizingEdge::*;
        let start = window(400, 300, 400, 200);
        // Width pulled by 100, height by 100: the height asks for the wider image (600).
        let pulled = Rect {
            left: start.left - 100,
            top: start.top - 100,
            ..start
        };
        let corner = constrain_sizing(pulled, TopLeft, 2.0, PAD, 96, WORK);
        assert_eq!((corner.right, corner.bottom), (start.right, start.bottom));
        assert_eq!(size(corner), (616, 316));
        let pulled = Rect {
            right: start.right + 300,
            bottom: start.bottom + 10,
            ..start
        };
        let corner = constrain_sizing(pulled, BottomRight, 2.0, PAD, 96, WORK);
        assert_eq!(corner, window(400, 300, 700, 350));
        for edge in [TopRight, BottomLeft] {
            let corner = constrain_sizing(start, edge, 2.0, PAD, 96, WORK);
            assert_eq!(size(corner), size(start), "{edge:?}");
        }
        let corner = constrain_sizing(start, TopRight, 2.0, PAD, 96, WORK);
        assert_eq!((corner.left, corner.bottom), (start.left, start.bottom));
        let corner = constrain_sizing(start, BottomLeft, 2.0, PAD, 96, WORK);
        assert_eq!((corner.right, corner.top), (start.right, start.top));
    }

    // Catches a post-it resized to nothing, past the screen, or moving its opposite edge.
    #[test]
    fn a_post_it_resizes_freely_within_its_minimum_and_the_work_area() {
        let work = rect(0, 0, 1000, 800);
        let start = window(100, 100, 300, 300);
        let wide = Rect {
            right: 900,
            bottom: 150,
            ..start
        };
        assert_eq!(
            constrain_free(wide, SizingEdge::BottomRight, 8, 96, work, None),
            Rect {
                right: 900,
                bottom: 100 + 96 + 16,
                ..start
            }
        );
        let past = Rect {
            left: -200,
            top: -50,
            ..start
        };
        assert_eq!(
            constrain_free(past, SizingEdge::TopLeft, 8, 96, work, None),
            Rect {
                left: 0,
                top: 0,
                ..start
            }
        );
        let tiny = Rect {
            left: start.right - 10,
            ..start
        };
        assert_eq!(
            constrain_free(tiny, SizingEdge::Left, 8, 96, work, None).left,
            start.right - 96 - 16
        );
    }

    // Catches a collapsed post-it's edges opening its window past the title bar, which showed
    // the hidden content or left an empty strip.
    #[test]
    fn a_collapsed_post_it_keeps_its_bar_height_from_every_edge() {
        use SizingEdge::*;
        let work = rect(0, 0, 1000, 800);
        let bar = collapsed_height(96, 8);
        let start = rect(100, 100, 316, bar);
        for edge in [
            Left,
            Right,
            Top,
            Bottom,
            TopLeft,
            TopRight,
            BottomLeft,
            BottomRight,
        ] {
            let pulled = Rect {
                left: start.left - 50,
                top: start.top - 50,
                right: start.right + 50,
                bottom: start.bottom + 50,
            };
            let sized = constrain_free(pulled, edge, 8, 96, work, Some(bar));
            assert_eq!(sized.bottom - sized.top, bar, "{edge:?}");
            let kept = if matches!(edge, Top | TopLeft | TopRight) {
                sized.bottom == pulled.bottom
            } else {
                sized.top == pulled.top
            };
            assert!(kept, "{edge:?}: {sized:?}");
        }
    }

    // Catches a collapsed post-it saved with its bar above the screen reopening out of reach,
    // its expanded rectangle alone being visible enough.
    #[test]
    fn a_collapsed_post_it_above_the_work_area_comes_back_on_screen() {
        let monitors = [monitor(0, 1920, 96)];
        let fitted = fit_collapsed(rect(100, -200, 300, 340), &monitors);
        assert_eq!(fitted, rect(100, 0, 300, 340));
    }

    // Catches a post-it collapsed at the bottom of the screen jumping up on every start, judged by
    // its expanded rectangle, mostly off the work area.
    #[test]
    fn a_collapsed_post_it_at_the_bottom_stays_where_it_was() {
        let monitors = [monitor(0, 1920, 96)];
        let saved = rect(100, 1040 - 44, 300, 340);
        assert_eq!(fit_collapsed(saved, &monitors), saved);
    }

    // Catches a collapsed post-it saving its bar's height, which reopens it expanded to nothing.
    #[test]
    fn a_collapsed_post_it_saves_its_expanded_height() {
        let window = rect(100, 100, 316, 44);
        assert_eq!(saved_rect(window, true, 340), rect(100, 100, 316, 340));
        assert_eq!(saved_rect(window, false, 340), window);
    }

    // Catches expanding a post-it collapsed near the bottom growing it off the screen.
    #[test]
    fn a_post_it_expanded_near_the_bottom_stays_in_the_work_area() {
        let work = rect(0, 0, 1920, 1040);
        let bar = collapsed_rect(rect(100, 1000, 316, 340), 96, 8);
        assert_eq!(expand_rect(bar, 340, work), rect(100, 700, 316, 340));
        assert_eq!(expand_rect(bar, 5000, work), rect(100, 0, 316, 1040));
    }

    #[test]
    fn the_image_never_shrinks_under_the_minimum() {
        use SizingEdge::*;
        let tiny = window(100, 100, 20, 10);
        assert_eq!(
            size(constrain_sizing(tiny, Right, 2.0, PAD, 96, WORK)),
            (192 + 16, 96 + 16)
        );
        let tall = window(100, 100, 10, 20);
        assert_eq!(
            size(constrain_sizing(tall, Bottom, 0.5, PAD, 96, WORK)),
            (96 + 16, 192 + 16)
        );
    }

    // Catches a resize taking the window past the monitor's work area.
    #[test]
    fn a_resize_stops_at_the_work_area() {
        use SizingEdge::*;
        let start = window(1400, 300, 400, 200);
        let pulled = Rect {
            right: start.right + 1000,
            ..start
        };
        let limited = constrain_sizing(pulled, Right, 2.0, PAD, 96, WORK);
        assert_eq!(limited.right, WORK.right);
        assert_eq!(size(limited), (520, 252 + 16));
        let pulled = Rect {
            bottom: start.bottom + 2000,
            ..start
        };
        let limited = constrain_sizing(pulled, Bottom, 2.0, PAD, 96, WORK);
        assert!(limited.right <= WORK.right && limited.bottom <= WORK.bottom);
    }

    // Regression: the shorter side's minimum took a 24:1 image past the monitor.
    #[test]
    fn the_work_area_wins_over_the_minimum_for_a_long_image() {
        use SizingEdge::*;
        let start = window(0, 300, 1200, 50);
        for edge in [Right, Bottom, BottomRight] {
            let sized = constrain_sizing(start, edge, 24.0, PAD, 96, WORK);
            assert!(sized.right <= WORK.right, "{edge:?}: {sized:?}");
            let (width, height) = size(sized);
            let ratio = f64::from(width - 2 * PAD) / f64::from(height - 2 * PAD);
            assert!((ratio - 24.0).abs() < 0.5, "{edge:?}: {ratio}");
        }
    }

    // Regression: rounding a skinny image's width up to one pixel before deriving its height took
    // it past the largest height.
    #[test]
    fn a_skinny_image_stays_within_the_largest_size() {
        let aspect = 1.0 / 2000.0;
        let (zoomed, at_max) = zoom_rect(window(800, 100, 1, 500), 5000.0, aspect, PAD, 96, WORK);
        assert_eq!(size(zoomed), (1 + 2 * PAD, 790 + 2 * PAD));
        assert!(at_max);
        let fitted = contain(window(0, 0, 400, 300), aspect, PAD);
        assert_eq!(size(fitted), (1 + 2 * PAD, 300 + 2 * PAD));
    }

    #[test]
    fn a_window_is_reshaped_to_its_image() {
        assert_eq!(
            contain(window(10, 10, 400, 400), 2.0, PAD),
            window(10, 10, 400, 200)
        );
        assert_eq!(
            contain(window(10, 10, 400, 400), 0.5, PAD),
            window(10, 10, 200, 400)
        );
    }

    #[test]
    fn zooming_stays_between_the_minimum_and_the_preview_share() {
        let start = window(800, 400, 400, 200);
        let (zoomed, at_max) = zoom_rect(start, 600.0, 2.0, PAD, 96, WORK);
        assert_eq!(size(zoomed), (616, 316));
        assert!(!at_max);
        // Around the same center.
        let center = |r: Rect| ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
        assert_eq!(center(zoomed), center(start));
        // 72% of 1920 is 1382; 76% of 1040 is 790, which allows 1580 at 2:1.
        let (zoomed, at_max) = zoom_rect(start, 5000.0, 2.0, PAD, 96, WORK);
        assert_eq!(size(zoomed), (1382 + 16, 691 + 16));
        assert!(at_max);
        let (zoomed, at_max) = zoom_rect(start, 1.0, 2.0, PAD, 96, WORK);
        assert_eq!(size(zoomed), (192 + 16, 96 + 16));
        assert!(!at_max);
    }

    // Catches zooming near a screen edge pushing the window off the work area.
    #[test]
    fn a_zoomed_window_stays_in_the_work_area() {
        let corner = window(1700, 900, 200, 100);
        let (zoomed, _) = zoom_rect(corner, 800.0, 2.0, PAD, 96, WORK);
        assert!(zoomed.left >= WORK.left && zoomed.right <= WORK.right);
        assert!(zoomed.top >= WORK.top && zoomed.bottom <= WORK.bottom);
    }

    #[test]
    fn the_largest_image_takes_the_preview_share_of_the_work_area() {
        assert_eq!(max_image_size(WORK), (1382, 790));
    }

    #[test]
    fn a_centered_window_takes_at_most_half_the_work_area() {
        let work = rect(0, 0, 1920, 1040);
        assert_eq!(centered(work, (400, 300), 96), rect(760, 370, 400, 300));
        assert_eq!(centered(work, (4000, 1000), 96), rect(480, 400, 960, 240));
    }

    #[test]
    fn drop_rect_centers_horizontally_on_cursor_and_clamps_to_work_area() {
        let work = rect(0, 0, 1920, 1040);
        let dropped = drop_rect(
            DropSpot {
                cursor: Point { x: 800, y: 400 },
                work,
                dpi: 96,
            },
            NOTE_SIZE,
            PAD,
        );
        assert_eq!(size(dropped), (300 + 16, 320 + 16));
        assert_eq!((dropped.left + dropped.right) / 2, 800);
        assert_eq!(dropped.top, 400 - 24 - PAD);

        // Near bottom-right corner at 150% DPI.
        let corner = drop_rect(
            DropSpot {
                cursor: Point { x: 1910, y: 1030 },
                work,
                dpi: 144,
            },
            NOTE_SIZE,
            12,
        );
        assert_eq!(corner.right, work.right);
        assert_eq!(corner.bottom, work.bottom);
        assert!(corner.left >= work.left && corner.top >= work.top);
    }

    #[test]
    fn drop_existing_rect_preserves_window_size() {
        let work = rect(0, 0, 1920, 1040);
        let existing = rect(100, 100, 450, 520);
        let moved = drop_existing_rect(
            DropSpot {
                cursor: Point { x: 600, y: 300 },
                work,
                dpi: 96,
            },
            existing,
            PAD,
        );
        assert_eq!(size(moved), (450, 520));
        assert_eq!((moved.left + moved.right) / 2, 600);
        assert_eq!(moved.top, 300 - 24 - PAD);
    }
}
