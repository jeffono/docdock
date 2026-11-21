//! Operating system-specific code. Currently Windows only.

pub mod autostart;
pub mod blocker;
pub mod browser;
mod capture;
mod clipboard;
pub mod edge;
pub mod external;
mod files;
#[cfg(debug_assertions)]
pub mod fonts;
mod guard;
pub mod keyboard;
mod memory;
pub mod permission;
pub mod power;
mod recycle;
pub mod spell;
pub mod taskbar;
mod windows;

pub use self::capture::exclude_from_capture;
pub use self::clipboard::write_private_text;
pub use self::files::{
    MAX_NAME, delete_on_close, fit_name, hold_ancestry, hold_directory, move_new, open_exclusive,
    open_folder, open_plain_file, replace_file,
};
pub use self::guard::guard;
pub use self::memory::set_memory_low;
pub use self::recycle::{recyclable, recycle};
pub use self::windows::{
    Activation, Monitor, OverlayWindow, PanelWindow, Point, Rect, alt_held, cursor_pos,
    drag_tolerance, escape_pressed, foreground_window, hide_window, monitors, place_windows,
    prepare_floating, primary_button_down, restore_focus, set_click_through, show_topmost,
    slide_windows, ui_language, wait_frame, window_rect,
};

/// Windows logical pixels are defined at 96 DPI.
pub const BASE_DPI: u32 = 96;

/// Logical pixels to physical pixels, truncating as the window layout does.
pub fn scale_dpi(dpi: u32, logical: u32) -> i32 {
    (logical * dpi / BASE_DPI) as i32
}

/// Reads a WebView2 string and releases its COM task allocation after conversion.
fn text(
    read: impl FnOnce(&mut ::windows::core::PWSTR) -> ::windows::core::Result<()>,
) -> ::windows::core::Result<String> {
    let mut value = ::windows::core::PWSTR::null();
    let result = read(&mut value);
    let value = webview2_com::CoTaskMemPWSTR::from(value);
    result?;
    Ok(value.to_string())
}

/// Waits off the WebView2 UI thread for installation or its asynchronous acknowledgement.
fn installed(
    receiver: std::sync::mpsc::Receiver<::windows::core::Result<()>>,
    timeout: Option<std::time::Duration>,
    label: &str,
    missing: &str,
) -> Result<(), crate::error::Error> {
    let result = match timeout {
        Some(timeout) => receiver.recv_timeout(timeout).ok(),
        None => receiver.recv().ok(),
    };
    match result {
        Some(Ok(())) => Ok(()),
        Some(Err(error)) => Err(crate::error::Error::unexpected(format!("{label}: {error}"))),
        None => Err(crate::error::Error::unexpected(missing)),
    }
}
