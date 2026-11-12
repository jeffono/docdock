//! Capture protection belongs to the top-level Tauri HWND, not WebView2's child HWNDs.

use tauri::WebviewWindow;
use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_MONITOR,
};

/// Called while each content window is still hidden, including recreated page windows.
/// Failures are diagnostic only: a capture API can never guarantee content protection.
pub fn exclude_from_capture(window: &WebviewWindow) {
    let hwnd = match window.hwnd() {
        Ok(hwnd) => hwnd,
        Err(error) => {
            eprintln!("[capture] {}: HWND unavailable: {error}", window.label());
            return;
        }
    };
    // RtlGetVersion reports the actual build independently of compatibility manifests.
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    let affinity = match unsafe { RtlGetVersion(&mut version) }.ok() {
        Ok(())
            if version.dwMajorVersion < 10
                || (version.dwMajorVersion == 10 && version.dwBuildNumber < 19041) =>
        {
            eprintln!(
                "[capture] {}: Windows before 10 2004; using WDA_MONITOR (black in captures)",
                window.label()
            );
            WDA_MONITOR
        }
        Ok(()) => WDA_EXCLUDEFROMCAPTURE,
        Err(error) => {
            eprintln!(
                "[capture] {}: Windows version unavailable: {error}; requesting WDA_EXCLUDEFROMCAPTURE (older Windows uses WDA_MONITOR)",
                window.label()
            );
            WDA_EXCLUDEFROMCAPTURE
        }
    };
    if let Err(error) = unsafe { SetWindowDisplayAffinity(hwnd, affinity) } {
        eprintln!(
            "[capture] {}: SetWindowDisplayAffinity({affinity:?}): {error}",
            window.label()
        );
        if affinity == WDA_EXCLUDEFROMCAPTURE {
            eprintln!(
                "[capture] {}: falling back to WDA_MONITOR (black in captures)",
                window.label()
            );
            if let Err(error) = unsafe { SetWindowDisplayAffinity(hwnd, WDA_MONITOR) } {
                eprintln!(
                    "[capture] {}: SetWindowDisplayAffinity(WDA_MONITOR): {error}",
                    window.label()
                );
            }
        }
    }
}
