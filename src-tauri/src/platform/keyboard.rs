//! Keyboard layout queries.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, MAPVK_VSC_TO_VK, MapVirtualKeyExW,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// The virtual key the foreground window's layout gives the key with scan code `scan`; 0 if none.
pub fn virtual_key(scan: u32) -> u32 {
    // SAFETY: plain queries; a null foreground window reads the calling thread's layout.
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        MapVirtualKeyExW(scan, MAPVK_VSC_TO_VK, Some(GetKeyboardLayout(thread)))
    }
}
