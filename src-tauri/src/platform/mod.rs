//! Operating system-specific code. Currently Windows only.

mod files;
mod windows;

pub use self::files::{delete_on_close, move_new, open_exclusive, replace_file};
pub use self::windows::{
    Activation, Monitor, PanelWindow, Point, Rect, cursor_pos, escape_pressed, foreground_window,
    monitors, restore_focus, slide_windows, ui_language, wait_frame,
};
