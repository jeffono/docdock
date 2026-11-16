//! The edge handle's window: a small native popup with per-pixel alpha, so it needs no WebView and
//! its curved edges blend with whatever is below. It never takes focus, and lets clicks through
//! until the panel thread makes it the open panel's tab ([`super::PanelWindow::set_click_through`]).
//! It lives on the main thread, whose event loop pumps its messages; the panel thread only places
//! it and changes how it looks, which this module redraws on the main thread.

use std::sync::{Mutex, OnceLock};

use super::Point;
use crate::dock::Side;
use crate::handle::{Press, Shape};
use crate::tab::{self, Frame, Look};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, SelectObject,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GWLP_HINSTANCE, GetSystemMetrics, GetWindowLongPtrW,
    IDC_ARROW, LoadCursorW, MA_NOACTIVATE, PostMessageW, RegisterClassW, SM_CXDRAG, SM_CYDRAG,
    ULW_ALPHA, UpdateLayeredWindow, WM_APP, WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WNDCLASSW, WS_DISABLED, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

/// Posted to redraw on the main thread after the look or frame changed.
const WM_REDRAW: u32 = WM_APP + 1;

struct Paint {
    /// `None` until the UI reports its tokens; the handle is not shown before.
    look: Option<Look>,
    frame: Frame,
}

static PAINT: Mutex<Paint> = Mutex::new(Paint {
    look: None,
    frame: Frame {
        dpi: 96,
        side: Side::Right,
        focused: false,
    },
});
static WINDOW: OnceLock<isize> = OnceLock::new();
static CLICK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
/// Where the primary button went down on the tab, in client coordinates.
static PRESS: Mutex<Option<Press>> = Mutex::new(None);

/// Creates the hidden, disabled and click-through window, owned by the panel window `owner` so it
/// stays above it. Call on the main thread. `click` runs on that thread when the tab is clicked.
pub fn create(
    owner: isize,
    click: impl Fn() + Send + Sync + 'static,
) -> windows::core::Result<isize> {
    let _ = CLICK.set(Box::new(click));
    unsafe {
        let owner = HWND(owner as *mut _);
        let instance = HINSTANCE(GetWindowLongPtrW(owner, GWLP_HINSTANCE) as *mut _);
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: w!("DocDockHandle"),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        let style =
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
        let hwnd = CreateWindowExW(
            style,
            w!("DocDockHandle"),
            w!("DocDock"),
            WS_POPUP | WS_DISABLED,
            0,
            0,
            0,
            0,
            Some(owner),
            None,
            Some(instance),
            None,
        )?;
        // As a layered window it shows nothing until `draw` gives it pixels, once the look arrives.
        let raw = hwnd.0 as isize;
        let _ = WINDOW.set(raw);
        Ok(raw)
    }
}

/// Whether the UI has reported its look: until then the handle stays hidden.
pub fn ready() -> bool {
    PAINT.lock().is_ok_and(|paint| paint.look.is_some())
}

/// The UI's tokens changed, as with the theme.
pub fn set_look(look: Look) {
    if let Ok(mut paint) = PAINT.lock() {
        paint.look = Some(look);
    }
    redraw();
}

/// The panel moved to a monitor or side: the tab scales with `dpi` and grows away from the panel.
pub fn set_frame(dpi: u32, side: Side) {
    update(|frame| Frame { dpi, side, ..frame });
}

/// The panel gained or lost focus: the outline follows its inner-edge band.
pub fn set_focused(focused: bool) {
    update(|frame| Frame { focused, ..frame });
}

fn update(change: impl FnOnce(Frame) -> Frame) {
    let Ok(mut paint) = PAINT.lock() else {
        return;
    };
    let frame = change(paint.frame);
    if frame == paint.frame {
        return;
    }
    paint.frame = frame;
    drop(paint);
    redraw();
}

/// Callers are on other threads: the window's own thread draws it.
fn redraw() {
    if let Some(&raw) = WINDOW.get() {
        let _ = unsafe { PostMessageW(Some(HWND(raw as *mut _)), WM_REDRAW, WPARAM(0), LPARAM(0)) };
    }
}

fn client_point(lparam: LPARAM) -> Point {
    Point {
        x: i32::from(lparam.0 as u16 as i16),
        y: i32::from((lparam.0 >> 16) as u16 as i16),
    }
}

/// The system drag tolerance, in pixels.
fn slop() -> Point {
    unsafe {
        Point {
            x: GetSystemMetrics(SM_CXDRAG),
            y: GetSystemMetrics(SM_CYDRAG),
        }
    }
}

/// A press and release on the tab without dragging: dragging it does nothing. Only on the tab
/// itself: the strip over the panel's edge looks like the panel, so a click there does nothing.
fn is_click(press: Press, release: Point) -> bool {
    let Ok(frame) = PAINT.lock().map(|paint| paint.frame) else {
        return false;
    };
    let shape = Shape::new(frame.dpi);
    let inside = shape.tab_columns(frame.side).contains(&release.x)
        && (0..shape.height).contains(&release.y);
    inside && press.is_click(release, slop())
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        // Clicking the tab never takes focus from the app the user is in.
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_REDRAW => {
            draw(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Ok(mut press) = PRESS.lock() {
                *press = Some(Press::new(client_point(lparam)));
            }
            unsafe { SetCapture(hwnd) };
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Ok(mut press) = PRESS.lock()
                && let Some(press) = press.as_mut()
            {
                press.moved(client_point(lparam), slop());
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            // Taken before releasing capture, which clears it.
            let press = PRESS.lock().ok().and_then(|mut press| press.take());
            let _ = unsafe { ReleaseCapture() };
            if let Some(press) = press
                && is_click(press, client_point(lparam))
                && let Some(click) = CLICK.get()
            {
                click();
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            if let Ok(mut press) = PRESS.lock() {
                *press = None;
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Shows the tab's pixels with per-pixel alpha, at the window's current position. The bitmap
/// also sets the window's size, the same [`crate::handle::window_rect`] gives the panel thread.
fn draw(hwnd: HWND) {
    let Some((look, frame)) = PAINT
        .lock()
        .ok()
        .and_then(|paint| Some((paint.look?, paint.frame)))
    else {
        return;
    };
    let Some(pixmap) = tab::draw(&look, frame) else {
        return;
    };
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Top-down rows, as the pixmap stores them.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let dc = CreateCompatibleDC(None);
        let mut bits = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DeleteDC(dc);
            return;
        };
        // Premultiplied RGBA to the premultiplied BGRA that layered windows take.
        let target = std::slice::from_raw_parts_mut(bits.cast::<u8>(), pixmap.data().len());
        let pixels = target.as_chunks_mut::<4>().0.iter_mut();
        for (to, &[red, green, blue, alpha]) in pixels.zip(pixmap.data().as_chunks::<4>().0) {
            *to = [blue, green, red, alpha];
        }
        let previous = SelectObject(dc, bitmap.into());
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
            ..Default::default()
        };
        let size = SIZE {
            cx: width,
            cy: height,
        };
        let origin = POINT::default();
        let result = UpdateLayeredWindow(
            hwnd,
            None,
            None,
            Some(&size),
            Some(dc),
            Some(&origin),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        if let Err(error) = result {
            eprintln!("[panel] UpdateLayeredWindow failed: {error}");
        }
        SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
    }
}
