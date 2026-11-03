//! Thin Win32 layer. All coordinates are physical pixels: the process uses per-monitor DPI
//! awareness (PMv2), enabled by tao when starting the event loop.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    SetWindowRgn,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetAsyncKeyState, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, GWL_EXSTYLE, GetCursorPos,
    GetForegroundWindow, GetShellWindow, GetWindowLongPtrW, HWND_TOPMOST, IsWindow, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows::core::BOOL;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Half-open rectangle: `right` and `bottom` are excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn contains(self, p: Point) -> bool {
        (self.left..self.right).contains(&p.x) && (self.top..self.bottom).contains(&p.y)
    }
}

impl From<RECT> for Rect {
    fn from(r: RECT) -> Self {
        Self {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    pub bounds: Rect,
    pub work: Rect,
    /// Effective monitor DPI.
    pub dpi: u32,
}

/// `None` when the cursor cannot be read, such as on the UAC screen.
pub fn cursor_pos() -> Option<Point> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok()?;
    Some(Point { x: p.x, y: p.y })
}

/// Whether Escape is held or was pressed since the last read. Read on every cursor poll so a short
/// press is not missed between reads and the "since last read" bit never goes stale.
pub fn escape_pressed() -> bool {
    unsafe { GetAsyncKeyState(i32::from(VK_ESCAPE.0)) as u16 & 0x8001 != 0 }
}

unsafe extern "system" fn collect_monitor(
    handle: HMONITOR,
    _: HDC,
    _: *mut RECT,
    list: LPARAM,
) -> BOOL {
    unsafe { &mut *(list.0 as *mut Vec<HMONITOR>) }.push(handle);
    true.into()
}

/// All connected monitors.
pub fn monitors() -> Vec<Monitor> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    let list = LPARAM(&raw mut handles as isize);
    let _ = unsafe { EnumDisplayMonitors(None, None, Some(collect_monitor), list) };
    handles.into_iter().filter_map(monitor_info).collect()
}

fn monitor_info(handle: HMONITOR) -> Option<Monitor> {
    unsafe {
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(handle, &mut info).ok().ok()?;
        let (mut dpi, mut unused) = (0, 0);
        GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused).ok()?;
        Some(Monitor {
            bounds: info.rcMonitor.into(),
            work: info.rcWork.into(),
            dpi,
        })
    }
}

/// Blocks until the next DWM composition so each frame falls on a vsync.
pub fn wait_frame() {
    let _ = unsafe { DwmFlush() };
}

/// Foreground window. `None` during system transitions.
pub fn foreground_window() -> Option<isize> {
    let raw = unsafe { GetForegroundWindow() }.0 as isize;
    (raw != 0).then_some(raw)
}

/// How a panel window receives focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Never by clicking: the UI requests it with [`PanelWindow::activate`].
    OnRequest,
    /// By clicking, like any window. Used by the web page, which does not talk to Rust.
    OnClick,
}

/// A panel window. Stores the HWND as an integer because the panel thread owns it.
#[derive(Debug, Clone, Copy)]
pub struct PanelWindow {
    raw: isize,
    /// Monitor bounds: columns outside them are clipped.
    clip: Rect,
    top: i32,
    width: i32,
    height: i32,
}

impl PanelWindow {
    /// Applies styles and shows the window at its final size and position `x` without activating it.
    pub fn prepare(raw: isize, rect: Rect, clip: Rect, x: i32, activation: Activation) -> Self {
        let mut window = Self {
            raw,
            clip,
            top: rect.top,
            width: 0,
            height: 0,
        };
        window.set_enabled(false);
        unsafe {
            let hwnd = window.hwnd();
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let style = (style | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
            let style = match activation {
                Activation::OnRequest => style | WS_EX_NOACTIVATE.0,
                Activation::OnClick => style,
            };
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        }
        window.place(rect, clip, x);
        window
    }

    /// Sizes the window to `rect` at `x`, above other windows, without activating it. Columns
    /// outside `clip` are cut.
    pub fn place(&mut self, rect: Rect, clip: Rect, x: i32) {
        self.clip = clip;
        self.top = rect.top;
        self.width = rect.right - rect.left;
        self.height = rect.bottom - rect.top;
        self.clip(x);
        let result = unsafe {
            SetWindowPos(
                self.hwnd(),
                Some(HWND_TOPMOST),
                x,
                self.top,
                self.width,
                self.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        };
        log_failure("SetWindowPos", result);
    }

    fn hwnd(&self) -> HWND {
        HWND(self.raw as *mut _)
    }

    pub fn exists(&self) -> bool {
        unsafe { IsWindow(Some(self.hwnd())) }.as_bool()
    }

    /// Returns to the top of the always-visible windows without activating. Owned windows follow.
    pub fn raise(&self) {
        let flags = SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE;
        let result = unsafe { SetWindowPos(self.hwnd(), Some(HWND_TOPMOST), 0, 0, 0, 0, flags) };
        log_failure("SetWindowPos", result);
    }

    /// Clips anything beyond the monitor edges so it never appears on a neighboring monitor.
    fn clip(&self, x: i32) {
        let left = (self.clip.left - x).clamp(0, self.width);
        let right = (self.clip.right - x).clamp(0, self.width);
        unsafe {
            let region = CreateRectRgn(left, 0, right, self.height);
            // The system takes ownership of the region only if the call succeeds.
            if SetWindowRgn(self.hwnd(), Some(region), false) == 0 {
                let _ = DeleteObject(region.into());
            }
        }
    }

    /// A disabled window cannot be activated. Without this, the off-screen panel inherits focus when
    /// the foreground app closes because Windows chooses the next window in Z order even with
    /// `WS_EX_NOACTIVATE`, sending keyboard input to an invisible window.
    pub fn set_enabled(&self, enabled: bool) {
        let _ = unsafe { EnableWindow(self.hwnd(), enabled) };
    }

    pub fn is_foreground(&self) -> bool {
        unsafe { GetForegroundWindow() }.0 as isize == self.raw
    }

    /// Whether Windows agreed to give the window focus.
    pub fn activate(&self) -> bool {
        let activated = unsafe { SetForegroundWindow(self.hwnd()) }.as_bool();
        eprintln!("[panel] activate: ok={activated}");
        activated
    }
}

/// Moves windows to `x` in one transaction so the page stays attached to the panel.
pub fn slide_windows(windows: &[PanelWindow], x: i32) {
    let flags = SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER;
    let moved = || unsafe {
        let mut batch = BeginDeferWindowPos(windows.len() as i32)?;
        for window in windows {
            window.clip(x);
            batch = DeferWindowPos(batch, window.hwnd(), None, x, window.top, 0, 0, flags)?;
        }
        EndDeferWindowPos(batch)
    };
    log_failure("DeferWindowPos", moved());
}

/// Restores focus to the saved window or, if it no longer exists, the desktop, so keyboard input
/// does not remain in a hidden panel.
pub fn restore_focus(previous: Option<isize>) {
    unsafe {
        let hwnd = previous
            .map(|raw| HWND(raw as *mut _))
            .filter(|&hwnd| IsWindow(Some(hwnd)).as_bool())
            .unwrap_or_else(|| GetShellWindow());
        let restored = SetForegroundWindow(hwnd).as_bool();
        eprintln!(
            "[panel] restore: window={:#x} ok={restored}",
            hwnd.0 as isize
        );
    }
}

fn log_failure(call: &str, result: windows::core::Result<()>) {
    if let Err(error) = result {
        eprintln!("[panel] {call} failed: {error}");
    }
}

/// The user's Windows display language, as a `LANGID`.
pub fn ui_language() -> u16 {
    unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() }
}
