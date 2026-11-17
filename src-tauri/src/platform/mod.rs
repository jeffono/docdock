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
pub mod permission;
mod recycle;
pub mod taskbar;
mod windows;

pub use self::capture::exclude_from_capture;
pub use self::clipboard::write_private_text;
pub use self::files::{
    MAX_NAME, delete_on_close, fit_name, hold_ancestry, hold_directory, move_new, open_exclusive,
    open_folder, paste_timestamp, replace_file,
};
pub use self::guard::guard;
pub use self::recycle::{recyclable, recycle};
pub use self::windows::{
    Activation, Monitor, PanelWindow, Point, Rect, cursor_pos, drag_tolerance, escape_pressed,
    foreground_window, monitors, place_windows, primary_button_down, restore_focus, slide_windows,
    ui_language, wait_frame,
};

/// Windows logical pixels are defined at 96 DPI.
pub const BASE_DPI: u32 = 96;

/// Logical pixels to physical pixels, truncating as the window layout does.
pub fn scale_dpi(dpi: u32, logical: u32) -> i32 {
    (logical * dpi / BASE_DPI) as i32
}

