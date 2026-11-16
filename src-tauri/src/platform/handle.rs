//! The edge handle's window: a small native popup painted with GDI, so it needs no WebView. It never
//! takes focus, and lets clicks through until the panel thread makes it the open panel's tab
//! ([`super::PanelWindow::set_click_through`]). It lives on the main thread, whose event loop pumps
//! its messages; the panel thread only places it and changes how it looks.

use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

use super::Point;
use crate::handle::Press;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush,
    DEFAULT_CHARSET, DEFAULT_PITCH, DeleteObject, EndPaint, FF_SWISS, FW_SEMIBOLD, FillRect,
    GetTextFaceW, GetTextMetricsW, HDC, HFONT, InvalidateRect, OUT_DEFAULT_PRECIS, PAINTSTRUCT,
    SelectObject, SetBkMode, SetTextAlign, SetTextColor, TA_BASELINE, TA_CENTER, TEXTMETRICW,
    TRANSPARENT, TextOutW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GWLP_HINSTANCE, GetClientRect, GetSystemMetrics,
    GetWindowLongPtrW, IDC_ARROW, LWA_ALPHA, LoadCursorW, MA_NOACTIVATE, RegisterClassW, SM_CXDRAG,
    SM_CYDRAG, SetLayeredWindowAttributes, WM_CAPTURECHANGED, WM_ERASEBKGND, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_PAINT, WNDCLASSW, WS_DISABLED, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{PCWSTR, w};

/// The app identity on the tab: the product name, never translated.
const LABEL: &str = "DocDock";
/// Font families tried, at most: the UI sends the whole `--font-sans` stack.
const MAX_FONTS: usize = 4;

/// Colors and font resolved from the UI's tokens for the current theme.
#[derive(Debug, Clone, Deserialize)]
pub struct Look {
    pub background: [u8; 3],
    pub line: [u8; 3],
    pub text: [u8; 3],
    /// Font families in order of preference.
    pub fonts: Vec<String>,
    /// Font size in logical pixels.
    pub size: f32,
}

struct Paint {
    /// `None` until the UI reports its tokens; the handle is not shown before.
    look: Option<Look>,
    dpi: u32,
    /// The tab touches the panel, or the screen edge, on its right.
    attached_right: bool,
}

static PAINT: Mutex<Paint> = Mutex::new(Paint {
    look: None,
    dpi: 96,
    attached_right: true,
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
        // A layered window draws nothing until given its attributes; it stays fully opaque.
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;
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
    repaint();
}

/// The panel moved to a monitor or side: the font scales with `dpi` and the open side faces away.
pub fn set_frame(dpi: u32, attached_right: bool) {
    let Ok(mut paint) = PAINT.lock() else {
        return;
    };
    if (paint.dpi, paint.attached_right) == (dpi, attached_right) {
        return;
    }
    paint.dpi = dpi;
    paint.attached_right = attached_right;
    drop(paint);
    repaint();
}

fn repaint() {
    if let Some(&raw) = WINDOW.get() {
        let _ = unsafe { InvalidateRect(Some(HWND(raw as *mut _)), None, false) };
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

/// A press and release on the tab without dragging: dragging it does nothing.
fn is_click(hwnd: HWND, press: Press, release: Point) -> bool {
    let mut client = RECT::default();
    let _ = unsafe { GetClientRect(hwnd, &mut client) };
    let inside = (client.left..client.right).contains(&release.x)
        && (client.top..client.bottom).contains(&release.y);
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
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
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
                && is_click(hwnd, press, client_point(lparam))
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

fn color([red, green, blue]: [u8; 3]) -> COLORREF {
    COLORREF(u32::from(red) | u32::from(green) << 8 | u32::from(blue) << 16)
}

fn fill(dc: HDC, rect: RECT, rgb: [u8; 3]) {
    unsafe {
        let brush = CreateSolidBrush(color(rgb));
        FillRect(dc, &rect, brush);
        let _ = DeleteObject(brush.into());
    }
}

/// Text rotated a quarter turn clockwise, read from top to bottom with the letters' tops facing
/// right.
fn rotated_font(face: Option<&str>, height: i32) -> HFONT {
    let name: Vec<u16> = face
        .unwrap_or_default()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        CreateFontW(
            height,
            0,
            2700,
            2700,
            FW_SEMIBOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            u32::from(DEFAULT_PITCH.0 | FF_SWISS.0),
            PCWSTR(name.as_ptr()),
        )
    }
}

/// The first installed family of `fonts`, as CSS would pick it; GDI otherwise substitutes any
/// font silently. Without one, the system's default sans serif.
fn pick_font(dc: HDC, fonts: &[String], height: i32) -> HFONT {
    for face in fonts.iter().take(MAX_FONTS) {
        let font = rotated_font(Some(face), height);
        let mut actual = [0u16; 32];
        unsafe {
            let previous = SelectObject(dc, font.into());
            GetTextFaceW(dc, Some(&mut actual));
            SelectObject(dc, previous);
        }
        let end = actual.iter().position(|&c| c == 0).unwrap_or(actual.len());
        if String::from_utf16_lossy(&actual[..end]).eq_ignore_ascii_case(face) {
            return font;
        }
        let _ = unsafe { DeleteObject(font.into()) };
    }
    rotated_font(None, height)
}

fn paint(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        if let Ok(state) = PAINT.lock()
            && let Some(look) = &state.look
        {
            draw(dc, client, look, state.dpi, state.attached_right);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

fn draw(dc: HDC, client: RECT, look: &Look, dpi: u32, attached_right: bool) {
    fill(dc, client, look.background);
    // A 1 px CSS line, as the panel draws its own border.
    let line = (dpi / 96).max(1) as i32;
    let outer = if attached_right {
        RECT {
            right: client.left + line,
            ..client
        }
    } else {
        RECT {
            left: client.right - line,
            ..client
        }
    };
    for side in [
        RECT {
            bottom: client.top + line,
            ..client
        },
        RECT {
            top: client.bottom - line,
            ..client
        },
        outer,
    ] {
        fill(dc, side, look.line);
    }
    let height = -((look.size * dpi as f32 / 96.0).round() as i32);
    let label: Vec<u16> = LABEL.encode_utf16().collect();
    unsafe {
        let font = pick_font(dc, &look.fonts, height);
        let previous = SelectObject(dc, font.into());
        let mut metrics = TEXTMETRICW::default();
        let _ = GetTextMetricsW(dc, &mut metrics);
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, color(look.text));
        SetTextAlign(dc, TA_CENTER | TA_BASELINE);
        // The letters rise to the right of the baseline: center their height, not the cell's.
        let rise = metrics.tmAscent - metrics.tmInternalLeading;
        let x = (client.left + client.right - rise) / 2;
        let y = (client.top + client.bottom) / 2;
        let _ = TextOutW(dc, x, y, &label);
        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
    }
}
