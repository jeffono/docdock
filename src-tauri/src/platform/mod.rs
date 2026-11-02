//! Operating system-specific code. Currently Windows only.

mod windows;

pub use self::windows::{
    Activation, Monitor, PanelWindow, Point, Rect, cursor_pos, escape_pressed, foreground_window,
    monitors, restore_focus, slide_windows, wait_frame,
};
