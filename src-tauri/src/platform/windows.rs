//! Thin Win32 layer. All coordinates are physical pixels: the process uses per-monitor DPI
//! awareness (PMv2), enabled by tao when starting the event loop.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR,
    HRGN, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect, RDW_ALLCHILDREN, RDW_FRAME,
    RDW_INVALIDATE, RGN_ERROR, RGN_OR, RedrawWindow, SetWindowRgn,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_LBUTTON, VK_MENU, VK_RBUTTON,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, GWL_EXSTYLE, GetCursorPos,
    GetForegroundWindow, GetShellWindow, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
    HWND_TOPMOST, IsIconic, IsWindow, SM_CXDRAG, SM_CYDRAG, SM_SWAPBUTTON, SW_HIDE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_NCDESTROY, WM_SIZING, WMSZ_BOTTOM,
    WMSZ_BOTTOMLEFT, WMSZ_BOTTOMRIGHT, WMSZ_LEFT, WMSZ_RIGHT, WMSZ_TOP, WMSZ_TOPLEFT,
    WMSZ_TOPRIGHT, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT,
};
use windows::core::BOOL;

use crate::handle::Outline;

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

/// Alt is held, but not as AltGr, which layouts such as ABNT2 send as Ctrl+Alt.
pub fn alt_held() -> bool {
    unsafe {
        GetAsyncKeyState(i32::from(VK_MENU.0)) < 0 && GetAsyncKeyState(i32::from(VK_CONTROL.0)) >= 0
    }
}

/// How far, in physical pixels each way, the pointer may move before a press becomes a drag.
pub fn drag_tolerance() -> Point {
    unsafe {
        Point {
            x: GetSystemMetrics(SM_CXDRAG),
            y: GetSystemMetrics(SM_CYDRAG),
        }
    }
}

unsafe extern "system" fn collect_monitor(
    handle: HMONITOR,
    _: HDC,
    _: *mut RECT,
    list: LPARAM,
) -> BOOL {
    // SAFETY: monitors passes an exclusive pointer to its live Vec; enumeration calls
    // this callback synchronously and never retains the pointer.
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
    /// The window draws the edge tab beside the panel, and shows and takes clicks only within this.
    outline: Option<Outline>,
}

impl PanelWindow {
    /// A window framed and shown by its caller's first [`place_windows`].
    fn new(raw: isize) -> Self {
        Self {
            raw,
            clip: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            top: 0,
            width: 0,
            height: 0,
            inset: 0,
            overlay_top: 0,
            offset: 0,
            outline: None,
        }
    }

    /// Applies styles and disables the still-hidden window. The caller must frame and place it.
    pub fn prepare(raw: isize, activation: Activation) -> Self {
        let window = Self::new(raw);
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
        window
    }

    /// Takes `rect` and `clip` for the next [`place_windows`], which applies them.
    pub fn set_rect(&mut self, rect: Rect, clip: Rect) {
        self.clip = clip;
        self.top = rect.top;
        self.width = rect.right - rect.left;
        self.height = rect.bottom - rect.top;
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

    /// Shapes the window to the panel and its tab from the next placement on.
    pub fn set_outline(&mut self, outline: Outline) {
        self.outline = Some(outline);
    }

    /// Reveal the panel border and its floating dropdown without resizing the page viewport.
    /// Stored here so sliding and monitor clipping preserve both cutouts.
    pub fn overlay(&mut self, dpi: u32, height: u32, x: i32) {
        // Rounded up, unlike `scale_dpi`, so the inset and cutout never fall short of the UI's.
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
            let region = match &self.outline {
                None => Some(CreateRectRgn(rect.left, rect.top, rect.right, rect.bottom))
                    .filter(|region| !region.is_invalid()),
                Some(outline) => outline_region(outline, rect),
            };
            // A null region would remove the clip altogether, letting the transparent strip take
            // clicks and the windows show on a neighboring monitor: keep the previous one instead.
            let Some(region) = region else {
                eprintln!("[panel] window region could not be built; keeping the previous one");
                return;
            };
            // The system takes ownership of the region only if the call succeeds.
            if SetWindowRgn(self.hwnd(), Some(region), true) == 0 {
                let _ = DeleteObject(region.into());
            }
        }
    }

    /// Lets every click through to the windows below, or takes them again.
    pub fn set_click_through(&self, through: bool) {
        set_click_through(self.raw, through);
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

/// A window shown beside the panel that never takes focus or a click: a tool window that never
/// activates, disabled and permanently click-through. Unlike [`PanelWindow`], it does not slide.
#[derive(Debug, Clone, Copy)]
pub struct OverlayWindow(isize);

impl OverlayWindow {
    /// Applies the styles to the still-hidden window.
    pub fn prepare(raw: isize) -> Self {
        let window = Self(raw);
        unsafe {
            let hwnd = window.hwnd();
            let _ = EnableWindow(hwnd, false);
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let added =
                WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0;
            let style = (style | added) & !WS_EX_APPWINDOW.0;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        }
        window
    }

    fn hwnd(&self) -> HWND {
        HWND(self.0 as *mut _)
    }

    /// Shows the window at `rect`, above other windows, without activating it.
    pub fn show(&self, rect: Rect) {
        show_topmost(self.0, rect);
    }

    pub fn hide(&self) {
        let _ = unsafe { ShowWindow(self.hwnd(), SW_HIDE) };
    }

    pub fn exists(&self) -> bool {
        unsafe { IsWindow(Some(self.hwnd())) }.as_bool()
    }
}

/// Lets every click through to the windows below, as Tauri's `set_ignore_cursor_events` does:
/// hit-testing skips only a layered window, so it becomes one meanwhile. Or takes clicks again.
pub fn set_click_through(raw: isize, through: bool) {
    unsafe {
        let hwnd = HWND(raw as *mut _);
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let flags = WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0;
        let style = if through {
            style | flags
        } else {
            style & !flags
        };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        let flags = RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_FRAME;
        let _ = RedrawWindow(Some(hwnd), None, None, flags);
    }
}

/// Makes the still-hidden window a floating one: a tool window, off the taskbar and Alt+Tab, that
/// never activates, so clicking or dragging it keeps the user's app, or a summoned panel, in the
/// foreground. Unlike [`OverlayWindow`], it stays enabled and takes clicks.
pub fn prepare_floating(raw: isize) {
    unsafe {
        let hwnd = HWND(raw as *mut _);
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let style = (style | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) & !WS_EX_APPWINDOW.0;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
    }
}

/// Shows the window at `rect`, above other windows, without activating it: Tauri's `show` and
/// `set_focus` would.
pub fn show_topmost(raw: isize, rect: Rect) {
    let flags = SWP_NOACTIVATE | SWP_SHOWWINDOW;
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let result = unsafe {
        SetWindowPos(
            HWND(raw as *mut _),
            Some(HWND_TOPMOST),
            rect.left,
            rect.top,
            width,
            height,
            flags,
        )
    };
    log_failure("SetWindowPos", result);
}

pub fn hide_window(raw: isize) {
    let _ = unsafe { ShowWindow(HWND(raw as *mut _), SW_HIDE) };
}

/// The window's rectangle on screen; `None` once it is gone.
pub fn window_rect(raw: isize) -> Option<Rect> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(HWND(raw as *mut _), &mut rect) }.ok()?;
    Some(rect.into())
}

/// The edge or corner a user drags to resize a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizingEdge {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl SizingEdge {
    fn from_wmsz(edge: u32) -> Option<Self> {
        Some(match edge {
            WMSZ_LEFT => Self::Left,
            WMSZ_RIGHT => Self::Right,
            WMSZ_TOP => Self::Top,
            WMSZ_BOTTOM => Self::Bottom,
            WMSZ_TOPLEFT => Self::TopLeft,
            WMSZ_TOPRIGHT => Self::TopRight,
            WMSZ_BOTTOMLEFT => Self::BottomLeft,
            WMSZ_BOTTOMRIGHT => Self::BottomRight,
            _ => return None,
        })
    }
}

/// Decides the window rectangle a resize may take: from the one proposed, the edge dragged, the
/// window's DPI and the work area of the monitor nearest the proposal.
pub type Sizing = Box<dyn Fn(Rect, SizingEdge, u32, Rect) -> Rect + Send>;

/// Lets `sizing` decide every rectangle while the user resizes `raw` (`WM_SIZING`). Must run on
/// the window's thread; the hook goes with the window.
pub fn constrain_sizing(raw: isize, sizing: Sizing) -> Result<(), String> {
    let data = Box::into_raw(Box::new(sizing));
    // SAFETY: the subclass owns this Box after success, until WM_NCDESTROY; on failure no
    // subclass owns it and it is reclaimed below.
    let added =
        unsafe { SetWindowSubclass(HWND(raw as _), Some(sizing_subclass), 0, data as usize) };
    if added.as_bool() {
        Ok(())
    } else {
        // SAFETY: installation failed, so ownership never passed to the subclass.
        drop(unsafe { Box::from_raw(data) });
        Err("failed to subclass the window".into())
    }
}

unsafe extern "system" fn sizing_subclass(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: `constrain_sizing` supplies a live Box<Sizing> as reference data, alive until
    // WM_NCDESTROY removes this subclass and reconstructs the Box exactly once.
    let sizing = data as *mut Sizing;
    match message {
        WM_SIZING => {
            let proposed = lparam.0 as *mut RECT;
            if let Some(edge) = SizingEdge::from_wmsz(wparam.0 as u32)
                && !proposed.is_null()
            {
                unsafe {
                    let rect = Rect::from(*proposed);
                    let dpi = GetDpiForWindow(window).max(1);
                    let monitor = MonitorFromRect(&*proposed, MONITOR_DEFAULTTONEAREST);
                    let work = monitor_info(monitor).map_or(rect, |monitor| monitor.work);
                    let decided = (*sizing)(rect, edge, dpi, work);
                    *proposed = RECT {
                        left: decided.left,
                        top: decided.top,
                        right: decided.right,
                        bottom: decided.bottom,
                    };
                }
                return LRESULT(1);
            }
        }
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(window, Some(sizing_subclass), id);
            drop(Box::from_raw(sizing));
        },
        _ => {}
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

/// The outline's rectangles cut to the visible columns of `visible`, as one region.
/// The outline clipped to `visible`, or `None` if any part of it could not be built.
fn outline_region(outline: &Outline, visible: Rect) -> Option<HRGN> {
    unsafe {
        let region = CreateRectRgn(0, 0, 0, 0);
        if region.is_invalid() {
            return None;
        }
        for rect in outline.rects() {
            let (left, right) = (rect.left.max(visible.left), rect.right.min(visible.right));
            if left >= right {
                continue;
            }
            let part = CreateRectRgn(left, rect.top, right, rect.bottom);
            let combined = !part.is_invalid()
                && CombineRgn(Some(region), Some(region), Some(part), RGN_OR) != RGN_ERROR;
            if !part.is_invalid() {
                let _ = DeleteObject(part.into());
            }
            if !combined {
                let _ = DeleteObject(region.into());
                return None;
            }
        }
        Some(region)
    }
}

/// Sizes and shows windows at `x`, in this order above other windows, in one transaction: separate
/// moves can reach the screen in different frames, leaving the tab behind a resized panel.
pub fn place_windows(windows: &[&PanelWindow], x: i32) {
    let flags = SWP_NOACTIVATE | SWP_SHOWWINDOW;
    let placed = || unsafe {
        let mut batch = BeginDeferWindowPos(windows.len() as i32)?;
        for window in windows {
            window.clip(x);
            let (left, top) = (x + window.offset, window.top);
            let (width, height) = (window.width, window.height);
            batch = DeferWindowPos(
                batch,
                window.hwnd(),
                Some(HWND_TOPMOST),
                left,
                top,
                width,
                height,
                flags,
            )?;
        }
        EndDeferWindowPos(batch)
    };
    log_failure("DeferWindowPos", placed());
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
            // A minimized window would take keystrokes while invisible, and restoring it would
            // undo the user's choice, so the desktop takes focus instead, as for a closed window.
            .filter(|&hwnd| IsWindow(Some(hwnd)).as_bool() && !IsIconic(hwnd).as_bool())
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
            outline: None,
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
