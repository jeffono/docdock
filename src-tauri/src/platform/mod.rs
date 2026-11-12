//! Operating system-specific code. Currently Windows only.

pub mod autostart;
pub mod browser;
pub mod edge;
mod files;
mod guard;
pub mod keyboard;
pub mod presence;
mod recycle;
mod windows;

pub use self::files::{
    MAX_NAME, delete_on_close, fit_name, hold_ancestry, hold_directory, move_new, open_exclusive,
    paste_timestamp, replace_file,
};
pub use self::guard::guard;
pub use self::recycle::recycle;
pub use self::windows::{
    Activation, Monitor, PanelWindow, Point, Rect, cursor_pos, escape_pressed, foreground_window,
    monitors, primary_button_down, restore_focus, slide_windows, ui_language, wait_frame,
};
