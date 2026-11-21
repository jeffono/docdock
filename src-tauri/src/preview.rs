//! The side image preview: a thumbnail shown larger in its own window beside the panel. Where it
//! goes is pure geometry here; the dock thread alone shows, places and hides its window, so it never
//! races the panel's slides. The window is a secondary app window
//! ([0014](../../docs/decisions/0014-app-windows.md)): it has no capability and hears which image
//! to show only through the IPC channel it hands over with `image_preview_state`.

use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::dock::{Request, Side};
use crate::error::Error;
use crate::i18n::Locale;
use crate::platform::{BASE_DPI, Point, Rect, scale_dpi};

pub const LABEL: &str = "image-preview";
/// Tells `main` that Rust hid the preview on its own, such as when the panel hides.
const CLOSED: &str = "image-preview-closed";

/// Logical pixels between the preview and the edge tab, so the tab is never covered.
const GAP: u32 = 8;
/// Logical pixels kept free at the work area's other edges.
const MARGIN: u32 = 24;
/// Below this many logical pixels of room beside the panel, the preview is not worth showing.
const MIN_ROOM: u32 = 120;
/// The most of the room beside the panel the preview takes, in percent of its width and height, so
/// a large image still leaves the screen around it in sight. Never under `MIN_ROOM`.
const MAX_WIDTH: u32 = 72;
const MAX_HEIGHT: u32 = 76;
/// The frame around the image, in logical pixels: padding on every side (`--space-2`), and the
/// callout arrow on the panel's side, as deep as the padding and twice as tall. `ImagePreview.svelte`
/// draws the same shape.
const FRAME_PAD: u32 = 8;
const ARROW_WIDTH: u32 = 8;
const ARROW_HALF: f64 = 8.0;
/// The frame's corner radius; the arrow stays clear of the corners by this and 2 more.
const RADIUS: f64 = 8.0;

/// Which image the preview shows, and whether a show is on its way.
#[derive(Debug, Default)]
struct Preview {
    /// Changes on every show, hide and close, so work started for an earlier turn is dropped.
    generation: u64,
    /// Changes only when `path` does: the preview window keeps the highest it hears of.
    version: u64,
    path: Option<String>,
    /// A show began and has not published yet.
    pending: bool,
    /// Where the frame's arrow points, from the window placed for the current show.
    side: Side,
    arrow_y: f64,
}

/// What a close ended.
#[derive(Debug, PartialEq)]
struct Closed {
    /// An image was published: the preview window must forget it.
    cleared: bool,
    /// An image was published or on its way: `main` must forget it too.
    active: bool,
}

impl Preview {
    /// Starts a show; its image is published only if `publish` gets this turn back.
    fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.pending = true;
        self.generation
    }

    /// Publishes `path` for the show started at `generation`, unless a later show, a hide or a close
    /// came since, or another notes folder was chosen since validation (`folder` then and now).
    fn publish(&mut self, generation: u64, folder: u64, folder_now: u64, path: String) -> bool {
        if generation != self.generation {
            return false;
        }
        self.pending = false;
        if folder != folder_now {
            return false;
        }
        self.path = Some(path);
        self.version += 1;
        true
    }

    /// Ends the turn and forgets the image.
    fn close(&mut self) -> Closed {
        self.generation += 1;
        let cleared = self.path.take().is_some();
        if cleared {
            self.version += 1;
        }
        let active = cleared || self.pending;
        self.pending = false;
        Closed { cleared, active }
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation == generation && self.path.is_some()
    }

    /// Takes where the window for the show started at `generation` was placed, if still current.
    fn place(&mut self, generation: u64, side: Side, arrow_y: f64) -> bool {
        if !self.is_current(generation) {
            return false;
        }
        (self.side, self.arrow_y) = (side, arrow_y);
        self.version += 1;
        true
    }

    fn update(&self) -> Update {
        Update {
            version: self.version,
            path: self.path.clone(),
            side: self.side,
            arrow_y: self.arrow_y,
        }
    }
}

struct Session {
    preview: Preview,
    /// The preview window's channel, from its latest load.
    channel: Option<Channel<Update>>,
}

impl Session {
    fn push(&self) {
        if let Some(channel) = &self.channel {
            // Paths are far below the size Tauri delivers without a fetch command.
            if let Err(error) = channel.send(self.preview.update()) {
                eprintln!("[preview] update not delivered: {error}");
            }
        }
    }
}

/// Never taken on the main thread: the dock thread holds it while it shows the window, which waits
/// for the main thread. Commands that take it are asynchronous for that reason.
static SESSION: Mutex<Session> = Mutex::new(Session {
    preview: Preview {
        generation: 0,
        version: 0,
        path: None,
        pending: false,
        side: Side::Right,
        arrow_y: 0.0,
    },
    channel: None,
});
/// Whether the window exists and is fully set up. Never taken on the main thread, which creates it.
static READY: Mutex<bool> = Mutex::new(false);

fn session() -> MutexGuard<'static, Session> {
    SESSION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What the preview window shows: the highest `version` wins, whichever way it arrived.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    version: u64,
    path: Option<String>,
    /// The panel's side, which the arrow faces.
    side: Side,
    /// The arrow's tip, in CSS pixels from the window's top.
    arrow_y: f64,
}

#[derive(Serialize)]
pub struct State {
    #[serde(flatten)]
    update: Update,
    locale: Locale,
}

/// A CSS viewport box `[x, y, width, height]` of the panel's window, whose client area starts at
/// `origin`, in physical pixels. Saturates instead of overflowing on absurd input.
pub fn to_physical(css: [i32; 4], origin: Point, dpi: u32) -> Rect {
    let scale = |value: i32| i64::from(value) * i64::from(dpi) / i64::from(BASE_DPI);
    let at = |base: i32, value: i64| {
        (i64::from(base) + value).clamp(i32::MIN.into(), i32::MAX.into()) as i32
    };
    let [x, y, width, height] = css;
    let (left, top) = (scale(x), scale(y));
    Rect {
        left: at(origin.x, left),
        top: at(origin.y, top),
        right: at(origin.x, left + scale(width.max(0))),
        bottom: at(origin.y, top + scale(height.max(0))),
    }
}

/// Where the preview window goes, and where its arrow points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// In physical pixels.
    pub rect: Rect,
    pub side: Side,
    /// The arrow's tip, in CSS pixels from the window's top.
    pub arrow_y: f64,
}

/// Where the preview goes: beside the open `panel` on its inner side, past the edge tab
/// (`tab_depth`), within `work` and a share of the room there, vertically centered on `thumb` as
/// far as that allows, with its arrow at the thumbnail's height. The image keeps its exact
/// proportions inside the frame and never grows past its natural size in CSS pixels, as the panel
/// would show it at 100%. Rectangles are in physical pixels. `None` when the image has no size or
/// there is too little room.
pub fn place(
    work: Rect,
    panel: Rect,
    side: Side,
    thumb: Rect,
    natural: (u32, u32),
    dpi: u32,
    tab_depth: i32,
) -> Option<Placement> {
    let (width, height) = natural;
    if width == 0 || height == 0 {
        return None;
    }
    let margin = scale_dpi(dpi, MARGIN);
    let gap = tab_depth + scale_dpi(dpi, GAP);
    let (left, right) = match side {
        Side::Right => (work.left + margin, panel.left - gap),
        Side::Left => (panel.right + gap, work.right - margin),
    };
    let (top, bottom) = (work.top + margin, work.bottom - margin);
    let (room_width, room_height) = (right - left, bottom - top);
    let room = scale_dpi(dpi, MIN_ROOM);
    if room_width < room || room_height < room {
        return None;
    }
    let share = |length: i32, percent: u32| (length * percent as i32 / 100).max(room);
    let (pad, arrow) = (scale_dpi(dpi, FRAME_PAD), scale_dpi(dpi, ARROW_WIDTH));
    let chrome = (2 * pad + arrow, 2 * pad);
    let inner_width = share(room_width, MAX_WIDTH) - chrome.0;
    let inner_height = share(room_height, MAX_HEIGHT) - chrome.1;
    let css = |pixels: u32| f64::from(pixels) * f64::from(dpi) / f64::from(BASE_DPI);
    let (full_width, full_height) = (css(width), css(height));
    let scale = (f64::from(inner_width) / full_width)
        .min(f64::from(inner_height) / full_height)
        .min(1.0);
    // `as` saturates, and each side stays within the room it was given.
    let w = ((full_width * scale).round() as i32).clamp(1, inner_width) + chrome.0;
    let h = ((full_height * scale).round() as i32).clamp(1, inner_height) + chrome.1;
    let x = match side {
        Side::Right => right - w,
        Side::Left => left,
    };
    let center = (i64::from(thumb.top) + i64::from(thumb.bottom)) / 2;
    let y = (center - i64::from(h / 2)).clamp(top.into(), (bottom - h).into()) as i32;
    let logical = |pixels: f64| pixels * f64::from(BASE_DPI) / f64::from(dpi);
    let window_height = logical(f64::from(h));
    let reach = RADIUS + ARROW_HALF + 2.0;
    let arrow_y = if window_height - reach < reach {
        window_height / 2.0
    } else {
        logical((center - i64::from(y)) as f64).clamp(reach, window_height - reach)
    };
    Some(Placement {
        rect: Rect {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        },
        side,
        arrow_y,
    })
}

/// Forgets the image and tells both windows, if one was shown or about to be. The caller hides the
/// window. Returns whether there was one.
pub fn dismiss(app: &AppHandle) -> bool {
    let mut session = session();
    let closed = session.preview.close();
    if closed.cleared {
        session.push();
    }
    // Even for a show still on its way, so `main` drops the shows it queued after it.
    if closed.active {
        let _ = app.emit_to("main", CLOSED, ());
    }
    closed.active
}

/// Hides the preview from outside the dock thread, such as when the notes folder goes away or
/// another is chosen.
pub fn close(app: &AppHandle) {
    dismiss(app);
    if let Some(requests) = app.try_state::<Sender<Request>>() {
        let _ = requests.send(Request::HidePreview);
    }
}

/// Whether the show started at `generation` is still the one to show.
pub fn is_current(generation: u64) -> bool {
    session().preview.is_current(generation)
}

/// Publishes `placement` and runs `show` with its rectangle, only if the show started at
/// `generation` is still current, without a close slipping in between; the window hears the
/// arrow's place with its new size. `show` must not take the session.
pub fn apply_placement(generation: u64, placement: Placement, show: impl FnOnce(Rect)) -> bool {
    let mut session = session();
    if !session
        .preview
        .place(generation, placement.side, placement.arrow_y)
    {
        return false;
    }
    session.push();
    show(placement.rect);
    true
}

/// Shows `image`, a path relative to the notes folder, beside its thumbnail at `thumb` (the
/// thumbnail's CSS box in `main`), creating the window on first use. Fails if a later show, a hide
/// or a close came meanwhile, or another folder was chosen.
pub fn show(
    app: &AppHandle,
    requests: &Sender<Request>,
    image: String,
    thumb: [i32; 4],
    natural: (u32, u32),
) -> Result<(), Error> {
    let generation = session().preview.begin();
    let folder = crate::notes::check_image(app, &image)?;
    ensure_window(app, requests)?;
    // Held while sending, so the dock thread never sees the request without its image.
    let mut session = session();
    let now = crate::notes::folder_choices();
    if !session.preview.publish(generation, folder, now, image) {
        return Err(Error::unexpected("image preview superseded"));
    }
    session.push();
    requests
        .send(Request::ShowPreview {
            generation,
            rect: thumb,
            natural,
        })
        .map_err(Error::unexpected)
}

/// Hides the preview at the UI's request; the UI already knows, so `main` is not told.
pub fn hide(requests: &Sender<Request>) {
    let mut session = session();
    if session.preview.close().cleared {
        session.push();
    }
    let _ = requests.send(Request::HidePreview);
}

/// The preview window's state when it loads; later changes go to `channel`, replacing the one it
/// handed over before a reload.
pub fn state(channel: Channel<Update>, locale: Locale) -> State {
    let mut session = session();
    session.channel = Some(channel);
    State {
        update: session.preview.update(),
        locale,
    }
}

/// One reusable window, created hidden and handed to the dock thread, which makes it a tool window
/// that never activates and lets every click through. Reused only once all of that succeeded; a
/// window whose setup failed is destroyed, so a later show builds it again.
fn ensure_window(app: &AppHandle, requests: &Sender<Request>) -> Result<(), Error> {
    let mut ready = READY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *ready && app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    *ready = false;
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("DocDock image")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .focused(false)
        .resizable(false);
    let window = crate::main_window::secondary_window(app, builder)?;
    let attached = window.hwnd().map_err(Error::unexpected).and_then(|hwnd| {
        requests
            .send(Request::AttachPreview(hwnd.0 as isize))
            .map_err(Error::unexpected)
    });
    if let Err(error) = attached {
        let _ = window.destroy();
        return Err(error);
    }
    *ready = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };
    const TAB: i32 = 20;

    fn panel(side: Side) -> Rect {
        match side {
            Side::Right => Rect { left: 1460, ..WORK },
            Side::Left => Rect { right: 460, ..WORK },
        }
    }

    fn thumb(top: i32) -> Rect {
        Rect {
            left: 0,
            top,
            right: 0,
            bottom: top + 104,
        }
    }

    fn placed(side: Side, top: i32, natural: (u32, u32), dpi: u32) -> Option<Placement> {
        place(WORK, panel(side), side, thumb(top), natural, dpi, TAB)
    }

    fn size(placement: Placement) -> (i32, i32) {
        let rect = placement.rect;
        (rect.right - rect.left, rect.bottom - rect.top)
    }

    // Catches the preview covering the edge tab, leaving the work area or its arrow missing the
    // thumbnail on the right side.
    #[test]
    fn right_side_sits_left_of_the_tab_pointing_at_the_thumbnail() {
        let placement = placed(Side::Right, 400, (400, 300), 96).unwrap();
        // 1460 - 20 (tab) - 8 (gap) = 1432; the image plus 8 of padding around and 8 of arrow,
        // centered on 452.
        assert_eq!(
            placement,
            Placement {
                rect: Rect {
                    left: 1008,
                    top: 294,
                    right: 1432,
                    bottom: 610
                },
                side: Side::Right,
                arrow_y: 158.0,
            }
        );
    }

    #[test]
    fn left_side_mirrors_the_right() {
        let placement = placed(Side::Left, 400, (400, 300), 96).unwrap();
        assert_eq!((placement.rect.left, placement.rect.right), (488, 912));
        assert_eq!((placement.side, placement.arrow_y), (Side::Left, 158.0));
    }

    // Catches a preview beside a thumbnail near the top or bottom leaving the margins, or its arrow
    // running into the frame's corners.
    #[test]
    fn stays_within_the_margins_with_the_arrow_clear_of_the_corners() {
        let placement = placed(Side::Right, -50, (400, 300), 96).unwrap();
        assert_eq!((placement.rect.top, placement.rect.bottom), (24, 340));
        assert_eq!(placement.arrow_y, 18.0);
        let placement = placed(Side::Right, 1000, (400, 300), 96).unwrap();
        assert_eq!((placement.rect.top, placement.rect.bottom), (700, 1016));
        assert_eq!(placement.arrow_y, 298.0);
    }

    // Catches the arrow measured in physical pixels on a scaled monitor.
    #[test]
    fn the_arrow_is_in_css_pixels() {
        let placement = placed(Side::Right, 400, (400, 300), 192).unwrap();
        let center = 452 - placement.rect.top;
        assert_eq!(placement.arrow_y, f64::from(center) / 2.0);
    }

    #[test]
    fn a_frame_too_short_for_the_arrow_centers_it() {
        let placement = placed(Side::Right, 400, (400, 1), 96).unwrap();
        assert_eq!(size(placement), (424, 17));
        assert_eq!(placement.arrow_y, 8.5);
    }

    // Catches a small image blurred by upscaling, or one ignoring the monitor's scale.
    #[test]
    fn small_images_show_at_their_natural_css_size_plus_the_frame() {
        let placement = placed(Side::Right, 400, (100, 50), 144).unwrap();
        // 12 of padding on each side and 12 of arrow at 150%.
        assert_eq!(size(placement), (150 + 24 + 12, 75 + 24));
    }

    // Catches a large image filling the room beside the panel or losing its proportions.
    #[test]
    fn large_images_take_at_most_a_share_of_the_room() {
        // Room: 1432 - 24 = 1408 wide, 1016 - 24 = 992 tall; 72% and 76% of it.
        let (w, h) = size(placed(Side::Right, 400, (4000, 2000), 96).unwrap());
        assert_eq!((w, h), (1013, 495 + 16));
        assert_eq!((w - 24, h - 16), (989, 495));
        let (w, h) = size(placed(Side::Right, 400, (1000, 4000), 96).unwrap());
        assert_eq!((w - 24, h - 16), (184, 737));
        assert_eq!(h, 753);
    }

    #[test]
    fn the_share_never_falls_below_the_minimum_room() {
        let work = Rect { right: 600, ..WORK };
        // 182 - 28 - 24 = 130 of room: 72% would be 93, under 120.
        let panel = Rect { left: 182, ..work };
        let placement = place(work, panel, Side::Right, thumb(0), (1000, 1000), 96, TAB).unwrap();
        assert_eq!(size(placement), (120, 96 + 16));
    }

    #[test]
    fn too_little_room_beside_the_panel_shows_nothing() {
        let narrow = Rect { right: 600, ..WORK };
        let panel = Rect {
            left: 140,
            ..narrow
        };
        // 140 - 28 - 24 = 88 pixels of room, under 120.
        let placement = place(narrow, panel, Side::Right, thumb(0), (400, 300), 96, TAB);
        assert_eq!(placement, None);
    }

    #[test]
    fn an_image_without_size_shows_nothing() {
        assert_eq!(placed(Side::Right, 0, (0, 300), 96), None);
        assert_eq!(placed(Side::Left, 0, (300, 0), 96), None);
    }

    // Catches a switch to another thumbnail keeping the old arrow, or a stale show moving it.
    #[test]
    fn only_the_current_show_moves_the_arrow() {
        let mut preview = Preview::default();
        let first = preview.begin();
        assert!(preview.publish(first, 0, 0, "a.png".into()));
        assert!(preview.place(first, Side::Right, 40.0));
        let version = preview.version;
        let second = preview.begin();
        assert!(!preview.place(first, Side::Left, 90.0));
        assert_eq!(preview.version, version);
        assert!(preview.publish(second, 0, 0, "b.png".into()));
        assert!(preview.place(second, Side::Left, 90.0));
        let update = preview.update();
        assert_eq!((update.side, update.arrow_y), (Side::Left, 90.0));
        assert!(update.version > version);
    }

    // Catches an opening paused between validation and publication bringing back an image after
    // the preview closed for a locked vault, or showing a same-named image from another folder.
    #[test]
    fn a_paused_opening_never_outlives_a_close_or_a_folder_change() {
        let mut preview = Preview::default();
        let generation = preview.begin();
        // The folder became unavailable meanwhile: the monitor closed the preview.
        assert!(preview.close().active);
        assert!(!preview.publish(generation, 0, 0, "a.png".into()));
        assert_eq!(preview.path, None);
        assert!(!preview.is_current(generation));

        let generation = preview.begin();
        // Another folder was chosen after validation.
        assert!(!preview.publish(generation, 0, 1, "a.png".into()));
        assert_eq!(preview.path, None);
    }

    #[test]
    fn only_the_latest_opening_publishes() {
        let mut preview = Preview::default();
        let first = preview.begin();
        let second = preview.begin();
        assert!(!preview.publish(first, 0, 0, "a.png".into()));
        assert!(preview.publish(second, 0, 0, "b.png".into()));
        assert!(preview.is_current(second));
        assert!(!preview.is_current(first));
        assert!(preview.close().cleared);
        assert!(!preview.is_current(second));
    }

    // Catches the window's state answer, read while a show was on its way, outranking that show's
    // image so the first image stays blank.
    #[test]
    fn a_state_read_before_publication_ranks_below_it() {
        let mut preview = Preview::default();
        let generation = preview.begin();
        let before = preview.update();
        assert!(preview.publish(generation, 0, 0, "a.png".into()));
        let published = preview.update();
        assert!(before.version < published.version);
        assert_eq!(before.path, None);
        // Read after publication, the answer is the published image itself.
        let after = preview.update();
        assert_eq!(after.path.as_deref(), Some("a.png"));
        assert_eq!(after.version, published.version);
    }

    // Catches `main` never hearing that a show still on its way was dismissed, which would let a
    // show it queued after it run.
    #[test]
    fn closing_a_show_on_its_way_tells_main_but_not_the_window() {
        let mut preview = Preview::default();
        let generation = preview.begin();
        let version = preview.version;
        assert_eq!(
            preview.close(),
            Closed {
                cleared: false,
                active: true
            }
        );
        assert_eq!(preview.version, version);
        assert!(!preview.publish(generation, 0, 0, "a.png".into()));
        // Nothing left to close.
        assert!(!preview.close().active);
    }

    // Catches the thumbnail's box landing elsewhere on a scaled monitor or overflowing.
    #[test]
    fn viewport_boxes_scale_from_the_window_origin() {
        let origin = Point { x: 1440, y: 40 };
        let rect = to_physical([10, -20, 104, 104], origin, 144);
        assert_eq!(
            rect,
            Rect {
                left: 1455,
                top: 10,
                right: 1611,
                bottom: 166
            }
        );
        let huge = to_physical([i32::MAX, i32::MAX, i32::MAX, i32::MAX], origin, 192);
        assert_eq!((huge.right, huge.bottom), (i32::MAX, i32::MAX));
    }
}
