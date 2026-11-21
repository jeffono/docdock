//! Pure rules of floating windows: what `.docdock/floating.json` holds, where a window may sit on
//! the monitors there are now, and when a click-through window shows its controls. `floating.rs`
//! runs the effects. Floating images (M16) are the first kind; post-its (M17) add `note`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::platform::{Monitor, Rect, scale_dpi};

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
/// How far each window opened after another moves down and right, in logical pixels.
const CASCADE: u32 = 24;
const LABEL_PREFIX: &str = "float-";

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

/// A click-through window lets clicks reach it while Alt is held over it, and keeps them through a
/// drag already started with Alt, until the button is released.
pub fn reveal_step(revealed: bool, alt_held: bool, cursor_over: bool, button_down: bool) -> bool {
    (alt_held && cursor_over) || (revealed && button_down)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Image,
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

    /// Moves or fades the window `id`, if it is still saved: a window closed meanwhile stays gone.
    pub fn update(&mut self, id: &str, rect: Rect, opacity: f64) {
        if let Some(entry) = self.entry_mut(id) {
            entry.set_rect(rect);
            entry.opacity = clamp_opacity(opacity);
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

/// `rect` moved down and right for the `index`th window opened beside the same place, so a new
/// window never hides one already there.
pub fn cascade(rect: Rect, index: usize, dpi: u32) -> Rect {
    let step = scale_dpi(dpi, CASCADE).saturating_mul(index as i32);
    Rect {
        left: rect.left.saturating_add(step),
        top: rect.top.saturating_add(step),
        right: rect.right.saturating_add(step),
        bottom: rect.bottom.saturating_add(step),
    }
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

    // Catches this build erasing what a later one saved, such as M17's post-its.
    #[test]
    fn entries_of_an_unknown_kind_or_shape_are_kept_in_place() {
        let note = serde_json::json!({ "id": ID, "kind": "note", "path": "a.md", "pinned": true });
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
        store.update(ID, rect(-50, 5, 400, 300), 0.3);
        store.update(&"b".repeat(32), rect(0, 0, 1, 1), 1.0);
        let saved: Vec<_> = store.entries().cloned().collect();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].rect(), rect(-50, 5, 400, 300));
        assert_eq!(saved[0].opacity, 0.3);
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

    #[test]
    fn later_windows_cascade_down_and_right() {
        assert_eq!(
            cascade(rect(100, 100, 50, 50), 2, 144),
            rect(172, 172, 50, 50)
        );
    }

    #[test]
    fn a_centered_window_takes_at_most_half_the_work_area() {
        let work = rect(0, 0, 1920, 1040);
        assert_eq!(centered(work, (400, 300), 96), rect(760, 370, 400, 300));
        assert_eq!(centered(work, (4000, 1000), 96), rect(480, 400, 960, 240));
    }
}
