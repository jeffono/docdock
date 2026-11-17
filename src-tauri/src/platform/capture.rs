//! Capture protection belongs to Tauri's top-level HWNDs, not WebView2's child HWNDs.

use tauri::WebviewWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_MONITOR,
};

/// Called while each content window is still hidden, including recreated page windows. Best effort:
/// failures are logged, since a capture API can never guarantee content protection.
pub fn exclude_from_capture(window: &WebviewWindow) {
    let label = window.label();
    let hwnd = match window.hwnd() {
        Ok(hwnd) => hwnd,
        Err(error) => {
            eprintln!("[capture] {label}: HWND unavailable: {error}");
            return;
        }
    };
    // Windows before 10 2004 refuses WDA_EXCLUDEFROMCAPTURE; WDA_MONITOR shows black instead.
    let Err(error) = (unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) }) else {
        return;
    };
    eprintln!("[capture] {label}: WDA_EXCLUDEFROMCAPTURE refused ({error}); using WDA_MONITOR");
    if let Err(error) = unsafe { SetWindowDisplayAffinity(hwnd, WDA_MONITOR) } {
        eprintln!("[capture] {label}: WDA_MONITOR refused: {error}");
    }
}
