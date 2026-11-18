//! Windows resume notifications for the top-level panel window, including while it is hidden.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    PBT_APMRESUMEAUTOMATIC, WM_NCDESTROY, WM_POWERBROADCAST,
};

type Callback = Box<dyn Fn()>;

/// Calls `resumed` on the window thread after sleep or hibernation.
pub fn on_resume(window: isize, resumed: impl Fn() + 'static) -> Result<(), &'static str> {
    let data = Box::into_raw(Box::new(Box::new(resumed) as Callback));
    // SAFETY: the subclass owns the callback until WM_NCDESTROY after a successful registration.
    if unsafe { SetWindowSubclass(HWND(window as _), Some(subclass), 0, data as usize) }.as_bool() {
        Ok(())
    } else {
        // SAFETY: registration failed, so the callback is still ours.
        drop(unsafe { Box::from_raw(data) });
        Err("failed to register for Windows resume notifications")
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
    // SAFETY: on_resume supplies a live callback on this window's thread, reclaimed once below.
    match message {
        WM_POWERBROADCAST if wparam.0 == PBT_APMRESUMEAUTOMATIC as usize => unsafe {
            (*callback)()
        },
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(window, Some(subclass), id);
            drop(Box::from_raw(callback));
        },
        _ => {}
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}
