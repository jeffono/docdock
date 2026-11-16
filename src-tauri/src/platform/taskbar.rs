//! The taskbar that holds the notification area: its theme, its small icon size and when they may
//! have changed.

use std::sync::OnceLock;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromWindow};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetSystemMetricsForDpi, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, RegisterWindowMessageW, SM_CXSMICON, USER_DEFAULT_SCREEN_DPI, WM_DISPLAYCHANGE,
    WM_DPICHANGED, WM_NCDESTROY, WM_SETTINGCHANGE,
};
use windows::core::w;

/// Whether the taskbar is light. This is the system theme, separate from the apps theme
/// (`AppsUseLightTheme`). A missing value means dark, the taskbar's original look.
pub fn light() -> bool {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&mut size),
        )
    };
    status.is_ok() && value != 0
}

/// The small icon size, in pixels, at the DPI of the monitor whose taskbar holds the notification
/// area. Without Explorer, the primary monitor's.
pub fn small_icon_size() -> u32 {
    unsafe {
        let tray = FindWindowW(w!("Shell_TrayWnd"), None).unwrap_or_default();
        let monitor = MonitorFromWindow(tray, MONITOR_DEFAULTTOPRIMARY);
        let (mut dpi, mut unused) = (0, 0);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused).is_err() {
            dpi = USER_DEFAULT_SCREEN_DPI;
        }
        GetSystemMetricsForDpi(SM_CXSMICON, dpi).max(0) as u32
    }
}

type Callback = Box<dyn Fn()>;

static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();

/// Calls `changed` on `window`'s thread whenever a setting, the display or a DPI changes, or
/// Explorer restarts (`TaskbarCreated`, after which the glyph may be stale). Theme changes arrive
/// as `WM_SETTINGCHANGE` ("ImmersiveColorSet"); a scale change on the taskbar's monitor usually
/// changes the work area too, but not always, so the caller also rechecks now and then.
/// `window` must be a top-level window, which receives these broadcasts even while hidden.
pub fn on_change(window: isize, changed: impl Fn() + 'static) -> Result<(), String> {
    TASKBAR_CREATED.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) });
    let data = Box::into_raw(Box::new(Box::new(changed) as Callback));
    let added = unsafe { SetWindowSubclass(HWND(window as _), Some(subclass), 0, data as usize) };
    if added.as_bool() {
        Ok(())
    } else {
        drop(unsafe { Box::from_raw(data) });
        Err("failed to subclass the window".into())
    }
}

unsafe extern "system" fn subclass(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    let callback = data as *mut Callback;
    match message {
        WM_SETTINGCHANGE | WM_DISPLAYCHANGE | WM_DPICHANGED => unsafe { (*callback)() },
        _ if Some(&message) == TASKBAR_CREATED.get() => unsafe { (*callback)() },
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(window, Some(subclass), id);
            drop(Box::from_raw(callback));
        },
        _ => {}
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}
