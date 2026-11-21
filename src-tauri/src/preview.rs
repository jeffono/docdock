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
/// Logical pixels kept free at the work area's edges.
const MARGIN: u32 = 12;
/// Below this many logical pixels of room beside the panel, the preview is not worth showing.
const MIN_ROOM: u32 = 120;

/// Which image the preview shows or is about to, and in which turn.
#[derive(Debug, Default)]
struct Preview {
    /// Changes on every show, hide and close, so work started for an earlier turn is dropped.
    generation: u64,
    path: Option<String>,
}

impl Preview {
    /// Starts a show; its image is published only if `publish` gets this turn back.
    fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    /// Publishes `path` for the show started at `generation`, unless a later show, a hide or a close
    /// came since, or another notes folder was chosen since validation (`folder` then and now).
    fn publish(&mut self, generation: u64, folder: u64, folder_now: u64, path: String) -> bool {
        if generation != self.generation || folder != folder_now {
            return false;
        }
        self.path = Some(path);
        true
    }

    /// Ends the turn and forgets the image, returning whether there was one.
    fn close(&mut self) -> bool {
        self.generation += 1;
        self.path.take().is_some()
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation == generation && self.path.is_some()
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
            let update = Update {
                generation: self.preview.generation,
                path: self.preview.path.clone(),
            };
            // Paths are far below the size Tauri delivers without a fetch command.
            if let Err(error) = channel.send(update) {
                eprintln!("[preview] update not delivered: {error}");
            }
        }
    }
}

static SESSION: Mutex<Session> = Mutex::new(Session {
    preview: Preview {
        generation: 0,
        path: None,
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

/// What the preview window shows: the latest `generation` wins, whichever way it arrived.
#[derive(Serialize, Clone)]
pub struct Update {
    generation: u64,
    path: Option<String>,
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

/// Where the preview goes, in physical pixels: beside the open `panel` on its inner side, past the
/// edge tab (`tab_depth`) and within `work`, vertically centered on `thumb` as far as `work` allows.
/// The image keeps its proportions and never grows past its natural size in CSS pixels, as the
/// panel would show it at 100%. `None` when the image has no size or there is too little room.
pub fn preview_rect(
    work: Rect,
    panel: Rect,
    side: Side,
    thumb: Rect,
    natural: (u32, u32),
    dpi: u32,
    tab_depth: i32,
) -> Option<Rect> {
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
    let room = scale_dpi(dpi, MIN_ROOM);
    if right - left < room || bottom - top < room {
        return None;
    }
    let css = |pixels: u32| f64::from(pixels) * f64::from(dpi) / f64::from(BASE_DPI);
    let (full_width, full_height) = (css(width), css(height));
    let scale = (f64::from(right - left) / full_width)
        .min(f64::from(bottom - top) / full_height)
        .min(1.0);
    // `as` saturates, and each side stays within the room it was given.
    let w = ((full_width * scale).round() as i32).clamp(1, right - left);
    let h = ((full_height * scale).round() as i32).clamp(1, bottom - top);
    let x = match side {
        Side::Right => right - w,
        Side::Left => left,
    };
    let center = (i64::from(thumb.top) + i64::from(thumb.bottom)) / 2;
    let y = (center - i64::from(h / 2)).clamp(top.into(), (bottom - h).into()) as i32;
    Some(Rect {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    })
}

/// Forgets the image and tells both windows, if one was shown or about to be. The caller hides the
/// window. Returns whether there was one.
pub fn dismiss(app: &AppHandle) -> bool {
    let mut session = session();
    if !session.preview.close() {
        return false;
    }
    session.push();
    let _ = app.emit_to("main", CLOSED, ());
    true
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
    if session.preview.close() {
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
        update: Update {
            generation: session.preview.generation,
            path: session.preview.path.clone(),
        },
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

    fn place(side: Side, top: i32, natural: (u32, u32), dpi: u32) -> Option<Rect> {
        preview_rect(WORK, panel(side), side, thumb(top), natural, dpi, TAB)
    }

    // Catches the preview covering the edge tab or leaving the work area on the right side.
    #[test]
    fn right_side_sits_left_of_the_tab_centered_on_the_thumbnail() {
        let rect = place(Side::Right, 400, (400, 300), 96).unwrap();
        // 1460 - 20 (tab) - 8 (gap) = 1432; centered on 452.
        assert_eq!(
            rect,
            Rect {
                left: 1032,
                top: 302,
                right: 1432,
                bottom: 602
            }
        );
    }

    #[test]
    fn left_side_mirrors_the_right() {
        let rect = place(Side::Left, 400, (400, 300), 96).unwrap();
        assert_eq!(
            rect,
            Rect {
                left: 488,
                top: 302,
                right: 888,
                bottom: 602
            }
        );
    }

    // Catches a preview beside a thumbnail near the top or bottom leaving the screen.
    #[test]
    fn stays_within_the_work_area_vertically() {
        let rect = place(Side::Right, -50, (400, 300), 96).unwrap();
        assert_eq!((rect.top, rect.bottom), (12, 312));
        let rect = place(Side::Right, 1000, (400, 300), 96).unwrap();
        assert_eq!((rect.top, rect.bottom), (728, 1028));
    }

    // Catches a small image blurred by upscaling, or one ignoring the monitor's scale.
    #[test]
    fn small_images_show_at_their_natural_css_size() {
        let rect = place(Side::Right, 400, (100, 50), 144).unwrap();
        assert_eq!((rect.right - rect.left, rect.bottom - rect.top), (150, 75));
    }

    // Catches a large image overflowing the room beside the panel or losing its proportions.
    #[test]
    fn large_images_shrink_to_fit_keeping_proportions() {
        let rect = place(Side::Right, 400, (4000, 2000), 96).unwrap();
        // Room: 1432 - 12 wide, 1016 tall; the width limits.
        assert_eq!((rect.left, rect.right), (12, 1432));
        assert_eq!(rect.bottom - rect.top, 710);
        let rect = place(Side::Right, 400, (1000, 4000), 96).unwrap();
        assert_eq!((rect.top, rect.bottom), (12, 1028));
        assert_eq!(rect.right - rect.left, 254);
    }

    #[test]
    fn too_little_room_beside_the_panel_shows_nothing() {
        let narrow = Rect { right: 600, ..WORK };
        let panel = Rect {
            left: 140,
            ..narrow
        };
        // 140 - 28 - 12 = 100 pixels of room, under 120.
        let rect = preview_rect(narrow, panel, Side::Right, thumb(0), (400, 300), 96, TAB);
        assert_eq!(rect, None);
    }

    #[test]
    fn an_image_without_size_shows_nothing() {
        assert_eq!(place(Side::Right, 0, (0, 300), 96), None);
        assert_eq!(place(Side::Left, 0, (300, 0), 96), None);
    }

    // Catches an opening paused between validation and publication bringing back an image after
    // the preview closed for a locked vault, or showing a same-named image from another folder.
    #[test]
    fn a_paused_opening_never_outlives_a_close_or_a_folder_change() {
        let mut preview = Preview::default();
        let generation = preview.begin();
        // The folder became unavailable meanwhile: the monitor closed the preview.
        preview.close();
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
        assert!(preview.close());
        assert!(!preview.is_current(second));
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
