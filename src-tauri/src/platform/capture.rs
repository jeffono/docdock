//! Capture protection belongs to top-level HWNDs: Tauri's (not WebView2's child HWNDs) and the
//! edge handle's, which shows the panel's edge.

use tauri::WebviewWindow;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_MONITOR,
};

/// Called while each content window is still hidden, including recreated page windows. Best effort:
/// failures are logged, since a capture API can never guarantee content protection.
pub fn exclude_from_capture(window: &WebviewWindow) {
    let label = window.label();
    match window.hwnd() {
        Ok(hwnd) => exclude_hwnd(hwnd, label),
        Err(error) => eprintln!("[capture] {label}: HWND unavailable: {error}"),
    }
}

/// [`exclude_from_capture`] for a window Tauri does not own.
pub fn exclude_hwnd(hwnd: HWND, label: &str) {
    // Windows before 10 2004 refuses WDA_EXCLUDEFROMCAPTURE; WDA_MONITOR shows black instead.
    let Err(error) = (unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) }) else {
        return;
    };
    eprintln!("[capture] {label}: WDA_EXCLUDEFROMCAPTURE refused ({error}); using WDA_MONITOR");
    if let Err(error) = unsafe { SetWindowDisplayAffinity(hwnd, WDA_MONITOR) } {
        eprintln!("[capture] {label}: WDA_MONITOR refused: {error}");
    }
}
