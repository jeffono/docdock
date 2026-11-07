//! Thin Win32 layer. All coordinates are physical pixels: the process uses per-monitor DPI
//! awareness (PMv2), enabled by tao when starting the event loop.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    SetWindowRgn,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, GWL_EXSTYLE, GetCursorPos,
    GetForegroundWindow, GetShellWindow, GetSystemMetrics, GetWindowLongPtrW, HWND_TOPMOST,
    IsWindow, SM_SWAPBUTTON, SW_HIDE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
    SWP_NOZORDER, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
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

/// The primary mouse button is held. `GetAsyncKeyState` reads physical buttons, so swapped buttons
/// make the right one primary.
pub fn primary_button_down() -> bool {
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let button = if swapped { VK_RBUTTON } else { VK_LBUTTON };
    unsafe { GetAsyncKeyState(i32::from(button.0)) < 0 }
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
    inset: i32,
    overlay_top: i32,
    /// Horizontal distance from the panel's left edge: windows slide together at the panel's `x`.
    offset: i32,
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
            inset: 0,
            overlay_top: 0,
            offset: 0,
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
                x + self.offset,
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

    pub fn hide(&self) {
        self.set_enabled(false);
        let _ = unsafe { ShowWindow(self.hwnd(), SW_HIDE) };
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

    /// Places this window `offset` pixels right of the panel's left edge from the next placement on.
    pub fn set_offset(&mut self, offset: i32) {
        self.offset = offset;
    }

    /// Reveal the panel border and its floating dropdown without resizing the page viewport.
    /// Stored here so sliding and monitor clipping preserve both cutouts.
    pub fn overlay(&mut self, dpi: u32, height: u32, x: i32) {
        self.inset = (2 * dpi).div_ceil(96) as i32;
        self.overlay_top = (height * dpi).div_ceil(96) as i32;
        self.clip(x);
    }

    fn visible_rect(&self, x: i32) -> Rect {
        let left = (self.clip.left - x).clamp(self.inset, self.width);
        let bottom = (self.height - self.inset).max(0);
        Rect {
            left,
            right: (self.clip.right - x)
                .clamp(0, self.width - self.inset)
                .max(left),
            top: self.overlay_top.min(bottom),
            bottom,
        }
    }

    /// Clips anything beyond the monitor edges so it never appears on a neighboring monitor.
    fn clip(&self, x: i32) {
        let rect = self.visible_rect(x + self.offset);
        unsafe {
            let region = CreateRectRgn(rect.left, rect.top, rect.right, rect.bottom);
            // The system takes ownership of the region only if the call succeeds.
            if SetWindowRgn(self.hwnd(), Some(region), true) == 0 {
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
            let left = x + window.offset;
            batch = DeferWindowPos(batch, window.hwnd(), None, left, window.top, 0, 0, flags)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    // Catches a slide frame restoring the page over the dropdown or focus band.
    #[test]
    fn page_cutouts_survive_sliding_across_the_monitor_edge() {
        let page = PanelWindow {
            raw: 0,
            clip: Rect {
                left: 0,
                top: 0,
                right: 1000,
                bottom: 800,
            },
            top: 88,
            width: 420,
            height: 712,
            inset: 3,
            overlay_top: 160,
            offset: 0,
        };
        assert_eq!(
            page.visible_rect(900),
            Rect {
                left: 3,
                top: 160,
                right: 100,
                bottom: 709
            }
        );
    }
}
